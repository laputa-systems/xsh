##! ICMP and ICMPv6 packets, kernel error-queue records, and the socket and
##! clock helpers that ping and the path tools share. Layouts stay in XSH; the
##! `linux` module only moves bytes.

## One ICMP echo socket. A datagram socket lets the kernel own the identifier
## and the checksum; a raw socket delivers the IPv4 header and filters nothing
## for us, so the caller checks the identifier it chose.
export type EchoSocket = {fd: Int, raw: Bool, family: Str}

## One received datagram with the ancillary values the tools read.
export type Received = {address: Str, data: Bytes, ttl: Int?, stamp_ns: Int?, flags: Int}

## A queued `sock_extended_err`: why the kernel or a router refused a packet.
## `port` is the destination port of the failed datagram, `ttl` the hop limit
## the error itself arrived with, and `stamp_ns` its kernel receive time.
export type QueuedError = {errno: Int, origin: Int, kind: Int, code: Int, info: Int, offender: Str, payload: Bytes, port: Int, ttl: Int?, stamp_ns: Int?}

## An ICMP echo reply, or another message that reached an echo socket.
export type Echo = {kind: Int, code: Int, identifier: Int, sequence: Int, payload: Bytes}

## A clock the interpreter cannot provide at microsecond resolution: the kernel
## stamps a loopback datagram on receipt, which is the only nanosecond reading
## of the wall clock reachable from a script. `fd` is -1 when loopback cannot
## be used, and readings then have millisecond resolution.
export type Clock = {fd: Int, target: Bytes}

const IPV4_ECHO_REPLY = 0
const IPV4_ECHO_REQUEST = 8
const IPV6_ECHO_REPLY = 129
const IPV6_ECHO_REQUEST = 128

## The RFC 1071 checksum of a message whose checksum field is zero.
export pure checksum(message: Bytes) -> Int {
  var sum = 0
  var index = 0
  let size = message.len()
  while index < size {
    let high = message.byte_at(index) ?? 0
    let low = message.byte_at(index + 1) ?? 0
    sum += high * 256 + low
    index += 2
  }
  while sum > 65535 {
    sum = sum.bit_and(65535) + sum / 65536
  }
  65535 - sum
}

## The echo request type for a family ("inet" or "inet6").
export pure request_type(family: Str) -> Int {
  if family == "inet6" { IPV6_ECHO_REQUEST } else { IPV4_ECHO_REQUEST }
}

## Build an echo request. A datagram socket rewrites the identifier and the
## checksum, and the kernel computes ICMPv6 checksums for every socket type, so
## only a raw IPv4 message carries a checksum computed here.
export pure echo_request(family: Str, identifier: Int, sequence: Int, payload: Bytes, checksummed: Bool) -> Result[Bytes, Error] {
  let head = bytes.concat([bytes.from_ints([request_type(family), 0, 0, 0])?, bytes.pack_be(identifier, 2)?, bytes.pack_be(sequence, 2)?])
  let message = bytes.concat([head, payload])
  if ! checksummed or family == "inet6" { return Ok(message) }
  Ok(bytes.concat([message.slice(0, 2), bytes.pack_be(checksum(message), 2)?, message.slice(4)]))
}

## The fill of an echo payload of `size` bytes. Without a pattern byte N
## holds N, the classic ping fill; a pattern repeats from the first byte.
export pure echo_fill(size: Int, pattern: Bytes) -> Result[Bytes, Error] {
  let period = pattern.len()
  let values = if period == 0 { [index.bit_and(255) for index in range(size)] } else { [pattern.byte_at(index % period) ?? 0 for index in range(size)] }
  bytes.from_ints(values)
}

## Overwrite the front of a fill with `stamp` when the payload is large
## enough to carry it, as ping carries its send time.
export pure echo_stamped(fill: Bytes, stamp: Bytes) -> Bytes {
  if fill.len() < stamp.len() { return fill }
  bytes.concat([stamp, fill.slice(stamp.len())])
}

## The pattern bytes of a `-p` argument: hexadecimal digit pairs, a trailing
## odd digit standing alone, at most sixteen bytes. Returns null for a
## non-hexadecimal character.
export pure pattern_bytes(text: Str) -> Bytes? {
  var values: List[Int] = []
  var index = 0
  let size = text.byte_len()
  while index < size and values.len() < 16 {
    let first = "0123456789abcdef".find(text.byte_slice(index, length: 1).lower())
    if first == null { return null }
    var value = first
    if index + 1 < size {
      let second = "0123456789abcdef".find(text.byte_slice(index + 1, length: 1).lower())
      if second == null { return null }
      value = first * 16 + second
    }
    values += [value]
    index += 2
  }
  match bytes.from_ints(values) {
    Ok(packed) => packed
    Err(_) => null
  }
}

## Offset of the ICMP message in a received IPv4 datagram: raw sockets include
## the IP header, whose length is the low nibble of its first byte in words.
export pure ip_header_length(data: Bytes) -> Int {
  (data.byte_at(0) ?? 0).bit_and(15) * 4
}

## Decode an ICMP or ICMPv6 message into its echo fields. Returns null when
## the message is too short to carry a header.
export pure parse_echo(message: Bytes) -> Echo? {
  if message.len() < 8 { return null }
  let identifier = bytes.unpack_be(message, 2, 4) ?? 0
  let sequence = bytes.unpack_be(message, 2, 6) ?? 0
  {kind: message.byte_at(0) ?? 0, code: message.byte_at(1) ?? 0, identifier: identifier, sequence: sequence, payload: message.slice(8)}
}

## The echo reply type for a family.
export pure reply_type(family: Str) -> Int {
  if family == "inet6" { IPV6_ECHO_REPLY } else { IPV4_ECHO_REPLY }
}

## True when an IPv4 ICMP message carries a valid checksum.
export pure checksum_ok(message: Bytes) -> Bool {
  checksum(message) == 0
}

pure hex_short(value: Int) -> Str {
  if value == 0 { return "0" }
  var text = ""
  var rest = value
  while rest > 0 {
    text = "0123456789abcdef".byte_slice(rest % 16, length: 1) + text
    rest = rest / 16
  }
  text
}

## Dotted-quad text of four address bytes.
export pure ipv4_text(raw: Bytes) -> Str {
  let parts = [f"{raw.byte_at(index) ?? 0}" for index in range(4)]
  parts.join(".")
}

## RFC 5952 text of sixteen address bytes: lowercase, the longest run of zero
## groups (two or more) compressed, and IPv4-mapped addresses in dotted form.
export pure ipv6_text(raw: Bytes) -> Str {
  let groups = [(raw.byte_at(index * 2) ?? 0) * 256 + (raw.byte_at(index * 2 + 1) ?? 0) for index in range(8)]
  if groups[0] == 0 and groups[1] == 0 and groups[2] == 0 and groups[3] == 0 and groups[4] == 0 and groups[5] == 65535 {
    return "::ffff:" + ipv4_text(raw.slice(12, 4))
  }
  var best_start = -1
  var best_length = 0
  var start = -1
  for index in range(8) {
    if groups[index] == 0 {
      if start < 0 { start = index }
      let length = index - start + 1
      if length > best_length { best_start = start; best_length = length }
    } else {
      start = -1
    }
  }
  if best_length < 2 { best_start = -1 }
  var text = ""
  var index = 0
  while index < 8 {
    if index == best_start {
      text += "::"
      index += best_length
      continue
    }
    if text != "" and ! text.ends_with(":") { text += ":" }
    text += hex_short(groups[index])
    index += 1
  }
  text
}

## The iputils wording for a destination-unreachable, redirect, time-exceeded
## or parameter-problem ICMP message. `info` is the MTU of "fragmentation
## needed", the pointer of a parameter problem, and the pointer of ICMPv6.
export pure icmp4_text(kind: Int, code: Int, info: Int) -> Str {
  match kind {
    0 => "Echo Reply"
    3 => match code {
      0 => "Destination Net Unreachable"
      1 => "Destination Host Unreachable"
      2 => "Destination Protocol Unreachable"
      3 => "Destination Port Unreachable"
      4 => f"Frag needed and DF set (mtu = {info})"
      5 => "Source Route Failed"
      6 => "Destination Net Unknown"
      7 => "Destination Host Unknown"
      8 => "Source Host Isolated"
      9 => "Destination Net Prohibited"
      10 => "Destination Host Prohibited"
      11 => "Destination Net Unreachable for Type of Service"
      12 => "Destination Host Unreachable for Type of Service"
      13 => "Packet filtered"
      14 => "Precedence Violation"
      15 => "Precedence Cutoff in effect"
      else => f"Dest Unreachable, Bad Code: {code}"
    }
    4 => "Source Quench"
    5 => match code {
      0 => "Redirect Network"
      1 => "Redirect Host"
      2 => "Redirect Type of Service and Network"
      3 => "Redirect Type of Service and Host"
      else => f"Redirect, Bad Code: {code}"
    }
    8 => "Echo Request"
    11 => match code {
      0 => "Time to live exceeded"
      1 => "Frag reassembly time exceeded"
      else => f"Time exceeded, Bad Code: {code}"
    }
    12 => f"Parameter problem: pointer = {info}"
    13 => "Timestamp"
    14 => "Timestamp Reply"
    15 => "Information Request"
    16 => "Information Reply"
    17 => "Address Mask Request"
    18 => "Address Mask Reply"
    else => f"Bad ICMP type: {kind}"
  }
}

pure unreachable6_reason(code: Int) -> Str {
  match code {
    0 => "No route"
    1 => "Administratively prohibited"
    2 => "Beyond scope of source address"
    3 => "Address unreachable"
    4 => "Port unreachable"
    5 => "Source address failed ingress/egress policy"
    6 => "Reject route to destination"
    else => f"Bad code({code})"
  }
}

pure exceeded6_reason(code: Int) -> Str {
  match code {
    0 => "Hop limit"
    1 => "Defragmentation failure"
    else => f"code {code}"
  }
}

pure problem6_reason(code: Int, pointer: Int) -> Str {
  let field = match code {
    0 => "Wrong header field "
    1 => "Unknown header "
    2 => "Unknown option "
    else => f"code {code} "
  }
  f"{field}at {pointer}"
}

## The iputils wording for an ICMPv6 error message.
export pure icmp6_text(kind: Int, code: Int, info: Int) -> Str {
  match kind {
    1 => f"Destination unreachable: {unreachable6_reason(code)}"
    2 => if code == 0 { f"Packet too big: mtu={info}" } else { f"Packet too big: mtu={info}, code={code}" }
    3 => f"Time exceeded: {exceeded6_reason(code)}"
    4 => f"Parameter problem: {problem6_reason(code, info)}"
    128 => "Echo request"
    129 => "Echo reply"
    else => f"unknown icmp type: {kind}"
  }
}

## Text of a socket address in a `sockaddr` byte image, or "" for a family
## this module does not read.
export pure sockaddr_text(raw: Bytes) -> Str {
  let family = bytes.unpack_le(raw, 2, 0) ?? 0
  if family == 2 and raw.len() >= 8 { return ipv4_text(raw.slice(4, 4)) }
  if family == 10 and raw.len() >= 24 { return ipv6_text(raw.slice(8, 16)) }
  ""
}

## Decode the `sock_extended_err` of an error-queue control message and the
## offender address that follows it.
export pure parse_queued_error(data: Bytes, payload: Bytes) -> QueuedError? {
  if data.len() < 16 { return null }
  let errno = bytes.unpack_le(data, 4, 0) ?? 0
  let info = bytes.unpack_le(data, 4, 8) ?? 0
  {errno: errno, origin: data.byte_at(4) ?? 0, kind: data.byte_at(5) ?? 0, code: data.byte_at(6) ?? 0, info: info, offender: sockaddr_text(data.slice(16)), payload: payload, port: 0, ttl: null, stamp_ns: null}
}

## Receive one datagram, reading the TTL or hop limit and the kernel receive
## timestamp from its ancillary data.
export proc receive(fd: Int, flags: Int = 0, max_bytes: Int = 65600) [process, net, error] -> Result[Received, Error] {
  let c = linux.net_constants()
  let got = linux.recvfrom(fd, max_bytes, flags)?
  var ttl: Int? = null
  var stamp: Int? = null
  for message in got.control {
    if message.level == c.SOL_IP and message.type == c.IP_TTL {
      ttl = bytes.unpack_le(message.data, 4)?
    } else if message.level == c.SOL_IPV6 and message.type == c.IPV6_HOPLIMIT {
      ttl = bytes.unpack_le(message.data, 4)?
    } else if message.level == c.SOL_SOCKET and message.type == c.SO_TIMESTAMPNS {
      stamp = bytes.unpack_le(message.data, 8, 0)? * 1000000000 + bytes.unpack_le(message.data, 8, 8)?
    }
  }
  Ok({address: got.address.address, data: got.data, ttl: ttl, stamp_ns: stamp, flags: got.flags})
}

## Take one entry off the socket's error queue, or null when it is empty.
## The datagram that caused the error is returned in `payload`.
export proc read_error(fd: Int) [process, net, error] -> Result[QueuedError?, Error] {
  let c = linux.net_constants()
  let got = match linux.recvfrom(fd, 65600, c.MSG_ERRQUEUE.bit_or(c.MSG_DONTWAIT)) {
    Ok(value) => value
    Err(failure) => {
      if failure.errno == 11 { return Ok(null) }
      return Err(failure)
    }
  }
  var found: QueuedError? = null
  var ttl: Int? = null
  var stamp: Int? = null
  for message in got.control {
    if message.level == c.SOL_SOCKET and message.type == c.SO_TIMESTAMPNS {
      stamp = bytes.unpack_le(message.data, 8, 0)? * 1000000000 + bytes.unpack_le(message.data, 8, 8)?
    } else if (message.level == c.SOL_IP and message.type == c.IP_TTL) or (message.level == c.SOL_IPV6 and message.type == c.IPV6_HOPLIMIT) {
      ttl = bytes.unpack_le(message.data, 4)?
    } else if (message.level == c.SOL_IP and message.type == c.IP_RECVERR) or (message.level == c.SOL_IPV6 and message.type == c.IPV6_RECVERR) {
      found = parse_queued_error(message.data, got.data)
    }
  }
  if let entry = found {
    return Ok({...entry, port: got.address.port, ttl: ttl, stamp_ns: stamp})
  }
  Ok(null)
}

## Open an ICMP echo socket for `family` ("inet" or "inet6"): a datagram socket
## first, which the kernel allows for the groups in `ping_group_range`, then a
## raw socket for processes with CAP_NET_RAW. `raw_only` skips the datagram
## socket, for callers that choose their own identifier. A failure carries the
## raw socket's errno, which is the one a privilege hint is about.
export proc open_echo(family: Str, raw_only: Bool = false) [process, net, error] -> Result[EchoSocket, Error] {
  let c = linux.net_constants()
  let domain = if family == "inet6" { c.AF_INET6 } else { c.AF_INET }
  let protocol = if family == "inet6" { c.IPPROTO_ICMPV6 } else { c.IPPROTO_ICMP }
  if ! raw_only {
    if let Ok(fd) = linux.socket(domain, c.SOCK_DGRAM, protocol) {
      return Ok({fd: fd, raw: false, family: family})
    }
  }
  let fd = linux.socket(domain, c.SOCK_RAW, protocol)?
  # A raw ICMP socket sees every ICMP message on the host: pass only echo
  # replies. A set bit blocks that type in both filter layouts.
  if family == "inet6" {
    let blocked = bytes.pack_le(4294967295, 4)?
    let reply_word = bytes.pack_le(4294967295 - 2, 4)?
    let filter = bytes.concat([blocked, blocked, blocked, blocked, reply_word, blocked, blocked, blocked])
    linux.setsockopt_bytes(fd, c.SOL_ICMPV6, c.ICMPV6_FILTER, filter)
  } else {
    linux.setsockopt_bytes(fd, c.SOL_RAW, c.ICMP_FILTER, bytes.pack_le(4294967294, 4)?)
  }
  Ok({fd: fd, raw: true, family: family})
}

## Ask the kernel for the TTL or hop limit, the receive timestamp, and queued
## ICMP errors on an echo socket.
export proc enable_ancillary(sock: EchoSocket) [process, error] -> Result[Unit, Error] {
  let c = linux.net_constants()
  linux.setsockopt_int(sock.fd, c.SOL_SOCKET, c.SO_TIMESTAMPNS, 1)
  if sock.family == "inet6" {
    linux.setsockopt_int(sock.fd, c.SOL_IPV6, c.IPV6_RECVHOPLIMIT, 1)
    linux.setsockopt_int(sock.fd, c.SOL_IPV6, c.IPV6_RECVERR, 1)
  } else {
    linux.setsockopt_int(sock.fd, c.SOL_IP, c.IP_RECVTTL, 1)
    linux.setsockopt_int(sock.fd, c.SOL_IP, c.IP_RECVERR, 1)
  }
  Ok()
}

proc bind_clock(fd: Int) [process, net, error] -> Result[Clock, Error] {
  let c = linux.net_constants()
  linux.bind(fd, {family: "inet", address: "127.0.0.1", port: 0})
  linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_TIMESTAMPNS, 1)
  linux.set_socket_timeout(fd, c.SO_RCVTIMEO, 1000)
  Ok({fd: fd, target: linux.getsockname(fd)?.raw})
}

## Open the nanosecond clock. Loopback being unusable (a namespace with `lo`
## down) leaves the millisecond wall clock, which makes round-trip times
## coarse rather than wrong.
export proc open_clock() [process, net, error] -> Result[Clock, Error] {
  let c = linux.net_constants()
  let coarse: Clock = {fd: -1, target: b""}
  guard let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM) else { return Ok(coarse) }
  match bind_clock(fd) {
    Ok(clock) => Ok(clock)
    Err(_) => {
      unix.close_fd(fd)
      Ok(coarse)
    }
  }
}

## Start a reading of the wall clock. The kernel stamps a loopback datagram
## when it is queued, so sending it immediately before a probe is the closest a
## script can come to the instant the probe leaves; `clock_collect` returns the
## stamp. A clock without a socket answers with the millisecond wall clock.
export proc clock_mark(clock: Clock) [process, net, time, error] -> Result[Int, Error] {
  if clock.fd < 0 { return Ok(time.now() * 1000000) }
  let _ = linux.sendto(clock.fd, b"x", clock.target)?
  Ok(-1)
}

## The nanosecond wall-clock reading that `clock_mark` started.
export proc clock_collect(clock: Clock, marked: Int) [process, net, time, error] -> Result[Int, Error] {
  if clock.fd < 0 { return Ok(marked) }
  let got = receive(clock.fd, 0, 16)?
  Ok(got.stamp_ns ?? time.now() * 1000000)
}

## The wall clock in nanoseconds now.
export proc clock_ns(clock: Clock) [process, net, time, error] -> Result[Int, Error] {
  let marked = clock_mark(clock)?
  clock_collect(clock, marked)
}
