#!/bin/xsh
use lib.gnu

const USAGE = """Usage: traceroute [OPTION]... HOST [PACKETLEN]
Print the route packets take to a network host, one line per hop.

  -4, -6                     use IPv4 or IPv6 only
  -I, --icmp                 probe with ICMP echo requests
  -T, --tcp                  probe with TCP SYN (needs a raw ICMP socket to
                             learn the hop addresses; default port 80)
  -U, --udp                  probe one UDP port, default 53
  -M, --module=NAME          method by name: default, udp, icmp, or tcp
  -O, --options=OPTS         method options, comma separated; "help" lists them
  -f, --first=TTL            start from hop TTL (default 1)
  -m, --max-hops=TTL         stop after hop TTL (default 30)
  -q, --queries=N            probes per hop, at most 10 (default 3)
  -N, --sim-queries=N        accepted; probes are always sent one at a time
  -w, --wait=MAX[,HERE,NEAR] seconds to wait for a reply: at most MAX, HERE
                             times the first answer of the same hop, or NEAR
                             times the previous answered hop's response
  -z, --sendwait=TIME        minimum interval between probes; above 10 it is
                             milliseconds, otherwise seconds
  -p, --port=PORT            first UDP port (incremented per probe), the fixed
                             port for -U and -T, or the first ICMP sequence
  -s, --source=ADDR          send from this source address
      --sport=PORT           send from this source port
  -i, --interface=NAME       send through this interface
  -g, --gateway=ADDR[,...]   loose source route through IPv4 gateways (max 8)
  -t, --tos=TOS              type of service or traffic class
  -F, --dont-fragment        set the do-not-fragment bit
  -r                         bypass the routing table
  -d, --debug                set SO_DEBUG on the probe sockets
      --fwmark=MARK          set the firewall mark of the probe sockets
  -n                         print addresses without resolving names
      --help                 display this help and exit
  -V, --version              output version information and exit

HOST is a name or an address; PACKETLEN is the whole packet length including
the IP header (default 60 for IPv4 and 80 for IPv6).
"""

type Options = {
  ipv4: Bool,
  ipv6: Bool,
  debug: Bool,
  dont_fragment: Bool,
  first: Str?,
  gateway: List[Str],
  icmp: Bool,
  tcp: Bool,
  udp: Bool,
  interface: Str?,
  max_hops: Str?,
  sim_queries: Str?,
  numeric: Bool,
  port: Str?,
  tos: Str?,
  wait: Str?,
  queries: Str?,
  bypass: Bool,
  source: Str?,
  sendwait: Str?,
  module: Str?,
  module_options: Str?,
  sport: Str?,
  fwmark: Str?,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

# Everything a trace needs after the command line is validated. `method` is
# "udp" (a new destination port per probe), "udp-fixed", "icmp", or "tcp".
# `icmp_sockets` is "any", "raw", or "dgram" and only matters to "icmp".
type Settings = {
  host: Str,
  target: Str,
  v6: Bool,
  method: Str,
  numeric: Bool,
  first_ttl: Int,
  max_ttl: Int,
  queries: Int,
  max_wait_ms: Int,
  here: Float,
  near: Float,
  send_wait_ms: Int,
  port: Int,
  source: Str?,
  interface: Str?,
  tos: Int?,
  dont_fragment: Bool,
  bypass: Bool,
  debug: Bool,
  fwmark: Int?,
  source_port: Int,
  gateways: List[Bytes],
  icmp_sockets: Str,
  payload_len: Int,
  packet_len: Int,
}

type Resolved = {addr: Str, v6: Bool}

# One answer to a probe. `mark` is the "!H"-style annotation printed after the
# time; `last` ends the trace after the hop's remaining probes.
type Reply = {from: Str, rtt_ms: Float, mark: Str, last: Bool}

type Control = {data: Bytes, level: Int, type: Int}

# A source of nanosecond timestamps. The runtime's own clock counts
# milliseconds, which is too coarse for a round trip, so the stamp comes from
# the kernel's receive timestamp of a datagram the script sends to a loopback
# socket of its own. `fd` is -1 when loopback is unusable, and stamps then
# fall back to the millisecond clock.
type Clock = {fd: Int, address: LinuxSocketAddress}

# One entry of a socket's extended error queue (struct sock_extended_err and
# the offender address that follows it).
type Queued = {origin: Int, kind: Int, code: Int, info: Int, offender: Str, port: Int, stamp: Int}

# What an ICMP message means for the trace. `known` is false for messages that
# are not an answer to a probe.
type Verdict = {known: Bool, mark: Str, last: Bool}

const FIRST_UDP_PORT = 33434
const FIXED_UDP_PORT = 53
const FIXED_TCP_PORT = 80
const MAX_PROBES_PER_HOP = 10
const WAIT_FLOOR_MS = 50
const HEX_DIGITS = "0123456789abcdef"
const ICMP_SOCKET_REFUSED = [1, 13, 93, 97]

pure pad2(value: Int) -> Str {
  if value < 10 { f" {value}" } else { f"{value}" }
}

pure hex_group(value: Int) -> Str {
  var rest = value
  var text = ""

  while true {
    let digit = rest % 16

    text = f"{HEX_DIGITS.byte_slice(digit, 1)}{text}"
    rest = rest / 16

    if rest == 0 {
      break
    }
  }

  text
}

pure ipv4_text(raw: Bytes, at: Int) -> Str {
  f"{raw.byte_at(at) ?? 0}.{raw.byte_at(at + 1) ?? 0}.{raw.byte_at(at + 2) ?? 0}.{raw.byte_at(at + 3) ?? 0}"
}

# The numeric form `getnameinfo` gives an IPv6 address: lower-case hex groups,
# the longest run of zero groups (two or more) compressed, and IPv4-mapped
# addresses with a dotted tail.
pure ipv6_text(raw: Bytes, at: Int) -> Str {
  var groups: List[Int] = []

  for index in range(8) {
    let high = raw.byte_at(at + index * 2) ?? 0
    let low = raw.byte_at(at + index * 2 + 1) ?? 0

    groups += [high * 256 + low]
  }

  let mapped = groups[0] == 0 and groups[1] == 0 and groups[2] == 0 and groups[3] == 0 and groups[4] == 0 and groups[5] == 65535

  return f"::ffff:{ipv4_text(raw, at + 12)}" when mapped

  var best_start = -1
  var best_len = 0
  var index = 0

  while index < 8 {
    if groups[index] == 0 {
      var end = index

      while end < 8 and groups[end] == 0 {
        end += 1
      }

      if end - index > best_len {
        best_len = end - index
        best_start = index
      }

      index = end
    } else {
      index += 1
    }
  }

  if best_len < 2 {
    best_start = -1
  }

  var text = ""
  index = 0

  while index < 8 {
    if index == best_start {
      text = f"{text}::"
      index += best_len
    } else {
      let separator = if text == "" or text.ends_with(":") { "" } else { ":" }

      text = f"{text}{separator}{hex_group(groups[index])}"
      index += 1
    }
  }

  text
}

# The RFC 1071 checksum of an ICMP message whose checksum field is zero.
pure icmp_checksum(message: Bytes) -> Int {
  var sum = 0
  var offset = 0

  while offset < message.len() {
    sum += (message.byte_at(offset) ?? 0) * 256 + (message.byte_at(offset + 1) ?? 0)
    offset += 2
  }

  while sum > 65535 {
    sum = sum.bit_and(65535) + sum / 65536
  }

  65535 - sum
}

# The traceroute payload pattern: bytes counting up from 0x40 modulo 64.
pure pattern(length: Int) -> Bytes {
  let values = [64 + index % 64 for index in range(length)]

  bytes.from_ints(values) ?? b""
}

pure unreachable_mark(code: Int, info: Int) -> Str {
  match code {
    0 => "!N"
    1 => "!H"
    2 => "!P"
    4 => f"!F-{info}"
    5 => "!S"
    6 => "!U"
    7 => "!W"
    8 => "!I"
    9 => "!A"
    10 => "!Z"
    11 => "!Q"
    12 => "!T"
    13 => "!X"
    14 => "!V"
    15 => "!C"
    _ => f"!<{code}>"
  }
}

pure unreachable_mark_v6(code: Int) -> Str {
  match code {
    0 => "!N"
    1 => "!X"
    2 => "!S"
    3 => "!H"
    _ => f"!<{code}>"
  }
}

# Interprets an ICMP type and code. Time exceeded names a hop that is not the
# destination; port unreachable (and an echo reply, handled by the caller)
# means the destination answered; the other unreachable codes end the trace
# with a mark.
pure classify(v6: Bool, kind: Int, code: Int, info: Int) -> Verdict {
  if v6 {
    return {known: true, mark: "", last: false} when kind == 3
    return {known: true, mark: "", last: true} when kind == 1 and code == 4
    return {known: true, mark: unreachable_mark_v6(code), last: true} when kind == 1
    return {known: true, mark: f"!F-{info}", last: true} when kind == 2
    return {known: false, mark: "", last: false}
  }

  return {known: true, mark: "", last: false} when kind == 11
  return {known: true, mark: "", last: true} when kind == 3 and code == 3
  return {known: true, mark: unreachable_mark(code, info), last: true} when kind == 3

  {known: false, mark: "", last: false}
}

pure bounded(text: Str, low: Int, high: Int) -> Int? {
  let value = text.parse_int_decimal() ?? low - 1

  if value < low or value > high {
    return null
  }

  value
}

# A nonnegative number of seconds, as milliseconds, or null.
pure seconds_ms(text: Str) -> Int? {
  let value = text.parse_float() ?? -1.0

  if value < 0.0 or value > 86400.0 {
    return null
  }

  let milliseconds = (value * 1000.0).round() ?? -1

  if milliseconds < 0 {
    return null
  }

  milliseconds
}

proc describe(what: Str, failure: Error) [error] -> Error {
  error.failure(f"{what}: {gnu.strerror(failure)}")
}

# Reports a command-line problem and ends with the status traceroute uses.
proc usage_die(message: Str) [process] -> Unit {
  eprint $message
  exit 2
}

# Milliseconds left before a deadline given in runtime-clock milliseconds.
proc remaining_ms(deadline_ms: Int) [time] -> Int {
  let left = deadline_ms - time.now()

  if left < 0 { 0 } else { left }
}

pure span_ms(start_ns: Int, end_ns: Int) -> Float {
  (end_ns - start_ns).float() / 1000000.0
}

# The kernel receive timestamp (SCM_TIMESTAMPNS, a timespec) among a
# message's ancillary data, in nanoseconds, or 0 when it carries none.
pure control_stamp(control: List[Control], level: Int, kind: Int) -> Int {
  let found = [entry for entry in control if entry.level == level and entry.type == kind]

  if found.is_empty() {
    return 0
  }

  let seconds = bytes.unpack_le(found[0].data, 8, 0) ?? 0
  let nanos = bytes.unpack_le(found[0].data, 8, 8) ?? 0

  seconds * 1000000000 + nanos
}

proc open_clock() [process, net, error] -> Clock {
  let c = linux.net_constants()
  let unusable: Clock = {fd: -1, address: {address: "", family: "unspec", port: 0, raw: b"", scope_id: 0}}

  guard let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM) else { |failure|
    return unusable
  }

  let bound = linux.bind(fd, {family: "inet", address: "127.0.0.1", port: 0})
  let stamped = linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_TIMESTAMPNS, 1)

  if bound is Err(_) or stamped is Err(_) {
    return unusable
  }

  guard let address = linux.getsockname(fd) else { |failure|
    return unusable
  }

  {fd: fd, address: address}
}

# The current time in nanoseconds on the same clock as kernel receive stamps.
proc stamp_ns(clock: Clock) [process, net, time, error] -> Int {
  let c = linux.net_constants()
  let coarse = time.now() * 1000000

  return coarse when clock.fd < 0

  if linux.sendto(clock.fd, b"t", clock.address) is Err(_) {
    return coarse
  }

  guard let message = linux.recvfrom(clock.fd, 1, c.MSG_DONTWAIT) else { |failure|
    return coarse
  }

  let stamped = control_stamp(message.control, c.SOL_SOCKET, c.SO_TIMESTAMPNS)

  if stamped == 0 { coarse } else { stamped }
}

# The receive time of a message: its kernel stamp, or a fresh clock reading.
proc receive_ns(clock: Clock, control: List[Control]) [process, net, time, error] -> Int {
  let c = linux.net_constants()
  let stamped = control_stamp(control, c.SOL_SOCKET, c.SO_TIMESTAMPNS)

  if stamped == 0 { stamp_ns(clock) } else { stamped }
}

proc resolve_one(name: Str, family: Str) [net, error] -> Result[Str] {
  let records = dns.resolve_host(name, family)?

  if records.is_empty() {
    return Err(error.failure(f"{name}: Name or service not known"))
  }

  Ok(records[0].addr)
}

# Resolves HOST honoring -4 and -6 (the last one given wins). A literal of the
# other family has no usable address, as for the reference tool.
proc resolve_host(name: Str, family: Str, argc: Int) [net, process, error] -> Result[Resolved] {
  guard let records = dns.resolve_host(name, family) else {
    let reason = if family == "any" { "Name or service not known" } else { "Name has no usable address" }

    eprint f"{name}: {reason}"
    eprint f"Cannot handle \"host\" cmdline arg `{name}' on position 1 (argc {argc})"
    exit 2
  }

  if records.is_empty() {
    eprint f"{name}: Name has no usable address"
    eprint f"Cannot handle \"host\" cmdline arg `{name}' on position 1 (argc {argc})"
    exit 2
  }

  Ok({addr: records[0].addr, v6: records[0].family == "ipv6"})
}

proc dotted_quad(name: Str) [net, error] -> Result[Bytes] {
  let addr = resolve_one(name, "ipv4")?
  var octets: List[Int] = []

  for part in addr.split(".") {
    octets += [part.parse_int_decimal()?]
  }

  bytes.from_ints(octets)
}

# The loose-source-route option for IP_OPTIONS: a no-op to align, the option
# header, then the gateways. The kernel appends the destination itself.
pure route_option(gateways: List[Bytes]) -> Bytes {
  let header = bytes.from_ints([1, 131, 3 + 4 * gateways.len(), 4]) ?? b""

  bytes.concat([header].extend(gateways))
}

proc socket_option(fd: Int, level: Int, name: Int, value: Int, label: Str) [process, error] -> Result[Unit] {
  if let Err(failure) = linux.setsockopt_int(fd, level, name, value) {
    return Err(describe(f"setsockopt {label}", failure))
  }

  Ok()
}

# Creates a probe socket and applies every option the command line selected.
proc open_socket(s: Settings, kind: Int, protocol: Int, recverr: Bool) [process, net, error] -> Result[Int] {
  let c = linux.net_constants()
  let family = if s.v6 { c.AF_INET6 } else { c.AF_INET }

  guard let fd = linux.socket(family, kind, protocol) else { |failure|
    return Err(describe("socket", failure))
  }

  let ip_level = if s.v6 { c.SOL_IPV6 } else { c.SOL_IP }

  if s.debug {
    socket_option(fd, c.SOL_SOCKET, c.SO_DEBUG, 1, "SO_DEBUG")?
  }

  if s.bypass {
    socket_option(fd, c.SOL_SOCKET, c.SO_DONTROUTE, 1, "SO_DONTROUTE")?
  }

  if let mark = s.fwmark {
    socket_option(fd, c.SOL_SOCKET, c.SO_MARK, mark, "SO_MARK")?
  }

  if let name = s.interface {
    let device = bytes.concat([bytes.from_text(name), b"\x00"])

    if let Err(failure) = linux.setsockopt_bytes(fd, c.SOL_SOCKET, c.SO_BINDTODEVICE, device) {
      return Err(describe("setsockopt SO_BINDTODEVICE", failure))
    }
  }

  if let tos = s.tos {
    socket_option(fd, ip_level, if s.v6 { c.IPV6_TCLASS } else { c.IP_TOS }, tos, "IP_TOS")?
  }

  if s.dont_fragment {
    if s.v6 {
      socket_option(fd, c.SOL_IPV6, c.IPV6_DONTFRAG, 1, "IPV6_DONTFRAG")?
    } else {
      socket_option(fd, c.SOL_IP, c.IP_MTU_DISCOVER, c.IP_PMTUDISC_DO, "IP_MTU_DISCOVER")?
    }
  }

  if ! s.gateways.is_empty() {
    if let Err(failure) = linux.setsockopt_bytes(fd, c.SOL_IP, c.IP_OPTIONS, route_option(s.gateways)) {
      return Err(describe("setsockopt IP_OPTIONS", failure))
    }
  }

  socket_option(fd, c.SOL_SOCKET, c.SO_TIMESTAMPNS, 1, "SO_TIMESTAMPNS")?

  if recverr {
    socket_option(fd, ip_level, if s.v6 { c.IPV6_RECVERR } else { c.IP_RECVERR }, 1, "IP_RECVERR")?
  }

  if s.source_port != 0 {
    socket_option(fd, c.SOL_SOCKET, c.SO_REUSEADDR, 1, "SO_REUSEADDR")?
  }

  if s.source != null or s.source_port != 0 {
    let wildcard = if s.v6 { "::" } else { "0.0.0.0" }
    let address = s.source ?? wildcard

    if let Err(failure) = linux.bind(fd, {family: if s.v6 { "inet6" } else { "inet" }, address: address, port: s.source_port}) {
      return Err(describe("bind", failure))
    }
  }

  Ok(fd)
}

proc set_ttl(s: Settings, fd: Int, ttl: Int) [process, error] -> Result[Unit] {
  let c = linux.net_constants()

  if s.v6 {
    socket_option(fd, c.SOL_IPV6, c.IPV6_UNICAST_HOPS, ttl, "IPV6_UNICAST_HOPS")
  } else {
    socket_option(fd, c.SOL_IP, c.IP_TTL, ttl, "IP_TTL")
  }
}

proc connect_target(s: Settings, fd: Int, port: Int) [process, net, error] -> Result[Unit] {
  let address = {family: if s.v6 { "inet6" } else { "inet" }, address: s.target, port: port}

  if let Err(failure) = linux.connect(fd, address) {
    return Err(describe("connect", failure))
  }

  Ok()
}

# Takes one entry off the extended error queue, or null when nothing is
# queued. Clearing the pending socket error keeps poll from reporting it again.
proc read_queue(s: Settings, fd: Int) [process, net, error] -> Result[Queued?] {
  let c = linux.net_constants()

  match linux.recvfrom(fd, 2048, c.MSG_ERRQUEUE.bit_or(c.MSG_DONTWAIT)) {
    Err(failure) => {
      if (failure.errno ?? 0) != 11 {
        return Err(describe("recvmsg", failure))
      }

      let _ = linux.getsockopt_int(fd, c.SOL_SOCKET, c.SO_ERROR)

      Ok(null)
    }
    Ok(message) => {
      let level = if s.v6 { c.SOL_IPV6 } else { c.SOL_IP }
      let kind = if s.v6 { c.IPV6_RECVERR } else { c.IP_RECVERR }
      let entries = [entry for entry in message.control if entry.level == level and entry.type == kind]

      if entries.is_empty() {
        return Ok(null)
      }

      let raw = entries[0].data
      let origin = raw.byte_at(4) ?? 0
      let offender = if s.v6 { ipv6_text(raw, 24) } else { ipv4_text(raw, 20) }

      Ok({
        origin: origin,
        kind: raw.byte_at(5) ?? 0,
        code: raw.byte_at(6) ?? 0,
        info: bytes.unpack_le(raw, 4, 12) ?? 0,
        offender: offender,
        port: message.address.port,
        stamp: control_stamp(message.control, c.SOL_SOCKET, c.SO_TIMESTAMPNS),
      })
    }
  }
}

# One UDP probe. The kernel queues the ICMP answer on the socket's error
# queue; the destination port of the quoted datagram names the probe.
proc udp_probe(s: Settings, clock: Clock, fd: Int, ttl: Int, port: Int, wait_ms: Int) [process, net, time, error] -> Result[Reply?] {
  set_ttl(s, fd, ttl)?
  connect_target(s, fd, port)?

  let start = stamp_ns(clock)

  if let Err(failure) = linux.sendto(fd, pattern(s.payload_len)) {
    return Err(describe("sendto", failure))
  }

  let deadline = time.now() + wait_ms

  while true {
    let events = unix.poll_fd(fd, [], remaining_ms(deadline))?

    if "error" in events {
      if let queued = read_queue(s, fd)? {
        let verdict = classify(s.v6, queued.kind, queued.code, queued.info)

        if queued.port == port and verdict.known {
          let end = if queued.stamp != 0 { queued.stamp } else { stamp_ns(clock) }

          return Ok({from: queued.offender, rtt_ms: span_ms(start, end), mark: verdict.mark, last: verdict.last})
        }
      }
    }

    if remaining_ms(deadline) == 0 {
      return Ok(null)
    }
  }

  Ok(null)
}

# An echo request without its identifier and checksum, for a datagram ICMP
# socket where the kernel fills both in.
pure echo_request(v6: Bool, seq: Int, ident: Int, checksum: Bool, payload: Bytes) -> Bytes {
  let kind = if v6 { 128 } else { 8 }
  let head = bytes.concat([bytes.from_ints([kind, 0, 0, 0]) ?? b"", bytes.pack_be(ident, 2) ?? b"", bytes.pack_be(seq, 2) ?? b""])
  let message = bytes.concat([head, payload])

  if ! checksum {
    return message
  }

  let sum = icmp_checksum(message)

  bytes.concat([message.slice(0, 2), bytes.pack_be(sum, 2) ?? b"", message.slice(4)])
}

# Whether the ICMP header at `at` is our echo request with this identifier
# and sequence (a quoted copy inside an error message, or an echo reply).
pure echo_matches(data: Bytes, at: Int, expected_type: Int, ident: Int, seq: Int) -> Bool {
  (data.byte_at(at) ?? -1) == expected_type and (bytes.unpack_be(data, 2, at + 4) ?? -1) == ident and (bytes.unpack_be(data, 2, at + 6) ?? -1) == seq
}

# One ICMP echo probe over a datagram ICMP socket: the answer arrives as an
# echo reply on the socket or as an entry on its error queue.
proc icmp_dgram_probe(s: Settings, clock: Clock, ttl: Int, seq: Int, wait_ms: Int) [process, net, time, error] -> Result[Reply?] {
  let c = linux.net_constants()
  let protocol = if s.v6 { c.IPPROTO_ICMPV6 } else { c.IPPROTO_ICMP }
  let fd = open_socket(s, c.SOCK_DGRAM, protocol, true)?

  defer unix.close_fd(fd)

  set_ttl(s, fd, ttl)?
  connect_target(s, fd, 0)?

  let start = stamp_ns(clock)
  let request = echo_request(s.v6, seq, 0, false, pattern(s.payload_len))

  if let Err(failure) = linux.sendto(fd, request) {
    return Err(describe("sendto", failure))
  }

  let reply_type = if s.v6 { 129 } else { 0 }
  let deadline = time.now() + wait_ms

  while true {
    let events = unix.poll_fd(fd, ["readable"], remaining_ms(deadline))?

    if "error" in events {
      if let queued = read_queue(s, fd)? {
        let verdict = classify(s.v6, queued.kind, queued.code, queued.info)

        if verdict.known {
          let end = if queued.stamp != 0 { queued.stamp } else { stamp_ns(clock) }

          return Ok({from: queued.offender, rtt_ms: span_ms(start, end), mark: verdict.mark, last: verdict.last})
        }
      }
    }

    if "readable" in events {
      match linux.recvfrom(fd, 65535) {
        Err(failure) => {
          if (failure.errno ?? 0) != 11 {
            return Err(describe("recvmsg", failure))
          }
        }
        Ok(message) => {
          if (message.data.byte_at(0) ?? -1) == reply_type and (bytes.unpack_be(message.data, 2, 6) ?? -1) == seq {
            let end = receive_ns(clock, message.control)

            return Ok({from: message.address.address, rtt_ms: span_ms(start, end), mark: "", last: true})
          }
        }
      }
    }

    if remaining_ms(deadline) == 0 {
      return Ok(null)
    }
  }

  Ok(null)
}

# Locates the quoted copy of a probe inside an ICMP error received on a raw
# socket: the offset just past the quoted IP header, or -1 when the message
# is not an error that quotes a packet. `at` is the start of the ICMP message.
pure quoted_start(v6: Bool, data: Bytes, at: Int) -> Int {
  let inner = at + 8

  if v6 {
    return inner + 40
  }

  let header = (data.byte_at(inner) ?? 0).bit_and(15) * 4

  if header < 20 {
    return -1
  }

  inner + header
}

# One raw-socket ICMP echo probe: the socket sees every ICMP message to this
# host, so each is matched on the identifier and sequence it carries.
proc icmp_raw_probe(s: Settings, clock: Clock, fd: Int, ident: Int, ttl: Int, seq: Int, wait_ms: Int) [process, net, time, error] -> Result[Reply?] {
  let c = linux.net_constants()
  let address = {family: if s.v6 { "inet6" } else { "inet" }, address: s.target, port: 0}

  set_ttl(s, fd, ttl)?

  let start = stamp_ns(clock)
  let request = echo_request(s.v6, seq, ident, ! s.v6, pattern(s.payload_len))

  if let Err(failure) = linux.sendto(fd, request, address) {
    return Err(describe("sendto", failure))
  }

  let echo_type = if s.v6 { 128 } else { 8 }
  let reply_type = if s.v6 { 129 } else { 0 }
  let deadline = time.now() + wait_ms

  while true {
    let events = unix.poll_fd(fd, ["readable"], remaining_ms(deadline))?

    if "readable" in events {
      match linux.recvfrom(fd, 65535, c.MSG_DONTWAIT) {
        Err(failure) => {
          if (failure.errno ?? 0) != 11 {
            return Err(describe("recvmsg", failure))
          }
        }
        Ok(message) => {
          let data = message.data
          let at = if s.v6 { 0 } else { (data.byte_at(0) ?? 0).bit_and(15) * 4 }
          let kind = data.byte_at(at) ?? -1
          let code = data.byte_at(at + 1) ?? 0

          if kind == reply_type and echo_matches(data, at, reply_type, ident, seq) {
            let end = receive_ns(clock, message.control)

            return Ok({from: message.address.address, rtt_ms: span_ms(start, end), mark: "", last: true})
          }

          let verdict = classify(s.v6, kind, code, bytes.unpack_be(data, 4, at + 4) ?? 0)

          if verdict.known {
            let quoted = quoted_start(s.v6, data, at)

            if quoted >= 0 and echo_matches(data, quoted, echo_type, ident, seq) {
              let end = receive_ns(clock, message.control)

              return Ok({from: message.address.address, rtt_ms: span_ms(start, end), mark: verdict.mark, last: verdict.last})
            }
          }
        }
      }
    }

    if remaining_ms(deadline) == 0 {
      return Ok(null)
    }
  }

  Ok(null)
}

# One TCP SYN probe from a fresh nonblocking socket. The connection result
# says the destination answered (an open port or a reset); intermediate hops
# and unreachables arrive as ICMP errors on the raw socket, matched on the
# local and destination ports the quoted TCP header carries.
proc tcp_probe(s: Settings, clock: Clock, raw_fd: Int, ttl: Int, port: Int, wait_ms: Int) [process, net, time, error] -> Result[Reply?] {
  let c = linux.net_constants()
  let fd = open_socket(s, c.SOCK_STREAM.bit_or(c.SOCK_NONBLOCK), 0, true)?

  defer unix.close_fd(fd)

  set_ttl(s, fd, ttl)?

  let address = {family: if s.v6 { "inet6" } else { "inet" }, address: s.target, port: port}
  let start = stamp_ns(clock)

  if let Err(failure) = linux.connect(fd, address) {
    let errno = failure.errno ?? 0

    if errno == 111 {
      return Ok({from: s.target, rtt_ms: span_ms(start, stamp_ns(clock)), mark: "", last: true})
    }

    if errno != 115 {
      return Err(describe("connect", failure))
    }
  }

  let local = linux.getsockname(fd)?.port
  let deadline = time.now() + wait_ms

  while true {
    let events = unix.poll_fd(fd, ["writable"], 0)?

    if "writable" in events or "error" in events or "hangup" in events {
      let pending = linux.getsockopt_int(fd, c.SOL_SOCKET, c.SO_ERROR)?

      if pending == 0 and "writable" in events {
        return Ok({from: s.target, rtt_ms: span_ms(start, stamp_ns(clock)), mark: "", last: true})
      }

      if pending == 111 {
        return Ok({from: s.target, rtt_ms: span_ms(start, stamp_ns(clock)), mark: "", last: true})
      }
    }

    let ready = unix.poll_fd(raw_fd, ["readable"], 1)?

    if "readable" in ready {
      match linux.recvfrom(raw_fd, 65535, c.MSG_DONTWAIT) {
        Err(failure) => {
          if (failure.errno ?? 0) != 11 {
            return Err(describe("recvmsg", failure))
          }
        }
        Ok(message) => {
          let data = message.data
          let at = if s.v6 { 0 } else { (data.byte_at(0) ?? 0).bit_and(15) * 4 }
          let kind = data.byte_at(at) ?? -1
          let verdict = classify(s.v6, kind, data.byte_at(at + 1) ?? 0, bytes.unpack_be(data, 4, at + 4) ?? 0)

          if verdict.known {
            let quoted = quoted_start(s.v6, data, at)

            if quoted >= 0 and (bytes.unpack_be(data, 2, quoted) ?? -1) == local and (bytes.unpack_be(data, 2, quoted + 2) ?? -1) == port {
              let end = receive_ns(clock, message.control)

              return Ok({from: message.address.address, rtt_ms: span_ms(start, end), mark: verdict.mark, last: verdict.last})
            }
          }
        }
      }
    }

    if remaining_ms(deadline) == 0 {
      return Ok(null)
    }
  }

  Ok(null)
}

# How long this probe may wait: the configured maximum, tightened to HERE
# times the first answer already seen at this hop or, for the first probe of
# a hop, NEAR times the previous answered hop's response. The floor keeps a
# loopback-fast neighbor from shrinking the wait below what a slower next hop
# needs.
pure probe_wait_ms(s: Settings, hop_rtt: Float, previous_rtt: Float) -> Int {
  var limit = s.max_wait_ms

  let scaled = if hop_rtt >= 0.0 {
    s.here * hop_rtt
  } else if previous_rtt >= 0.0 {
    s.near * previous_rtt
  } else {
    -1.0
  }

  if scaled >= 0.0 {
    let derived = scaled.round() ?? WAIT_FLOOR_MS
    let floored = if derived < WAIT_FLOOR_MS { WAIT_FLOOR_MS } else { derived }

    if floored < limit {
      limit = floored
    }
  }

  limit
}

proc address_text(s: Settings, addr: Str) [net] -> Str {
  return f" {addr}" when s.numeric

  let names = dns.reverse(addr) ?? []

  if names.is_empty() {
    return f" {addr} ({addr})"
  }

  f" {names[0]} ({addr})"
}

# Chooses the ICMP socket type once, before the first line of the trace, so a
# permission problem is reported once instead of at every probe.
proc pick_icmp_sockets(s: Settings) [process, net, error] -> Result[Str] {
  let c = linux.net_constants()
  let protocol = if s.v6 { c.IPPROTO_ICMPV6 } else { c.IPPROTO_ICMP }

  if s.icmp_sockets != "raw" {
    match linux.socket(if s.v6 { c.AF_INET6 } else { c.AF_INET }, c.SOCK_DGRAM, protocol) {
      Ok(fd) => {
        unix.close_fd(fd)?

        return Ok("dgram")
      }
      Err(failure) => {
        if s.icmp_sockets == "dgram" or (failure.errno ?? 0) not in ICMP_SOCKET_REFUSED {
          return Err(describe("socket", failure))
        }
      }
    }
  }

  Ok("raw")
}

proc run_trace(s: Settings, raw_fd: Int, icmp_mode: Str) [process, env, io, time, net, error] -> Result[Unit] {
  let c = linux.net_constants()
  let ident = (process.current_pid() ?? 1).bit_and(65535)
  let udp_fd = if s.method == "udp" or s.method == "udp-fixed" { open_socket(s, c.SOCK_DGRAM, 0, true)? } else { -1 }
  let clock = open_clock()
  var port = s.port
  var seq = s.port
  var last_send = 0
  var previous_rtt = -1.0
  var finished = false
  var ttl = s.first_ttl

  while ttl <= s.max_ttl and ! finished {
    var hop_rtt = -1.0
    var shown = ""
    var started = false

    for _ in range(s.queries) {
      if s.send_wait_ms > 0 and last_send != 0 {
        let idle = time.now() - last_send

        if idle < s.send_wait_ms {
          time.sleep(time.millis(s.send_wait_ms - idle))?
        }
      }

      last_send = time.now()

      let limit_ms = probe_wait_ms(s, hop_rtt, previous_rtt)
      let answer = match s.method {
        "udp" => udp_probe(s, clock, udp_fd, ttl, port, limit_ms)?
        "udp-fixed" => udp_probe(s, clock, udp_fd, ttl, port, limit_ms)?
        "icmp" => if icmp_mode == "dgram" { icmp_dgram_probe(s, clock, ttl, seq, limit_ms)? } else { icmp_raw_probe(s, clock, raw_fd, ident, ttl, seq, limit_ms)? }
        _ => tcp_probe(s, clock, raw_fd, ttl, port, limit_ms)?
      }

      # The hop number waits for the first probe to be sent, so a socket
      # error reads as it does for the reference tool: the header, then the
      # message.
      if ! started {
        gnu.write_text(f"\n{pad2(ttl)} ")
        started = true
      }

      if s.method == "udp" {
        port += 1
      }

      seq = (seq + 1).bit_and(65535)

      if let reply = answer {
        if reply.from != shown {
          gnu.write_text(address_text(s, reply.from))
          shown = reply.from
        }

        gnu.write_text(f"  {reply.rtt_ms.format(3)} ms")

        if reply.mark != "" {
          gnu.write_text(f" {reply.mark}")
        }

        if hop_rtt < 0.0 {
          hop_rtt = reply.rtt_ms
        }

        if reply.last {
          finished = true
        }
      } else {
        gnu.write_text(" *")
      }
    }

    if hop_rtt >= 0.0 {
      previous_rtt = hop_rtt
    }

    ttl += 1
  }

  gnu.write_text("\n")

  if udp_fd >= 0 {
    unix.close_fd(udp_fd)?
  }

  if clock.fd >= 0 {
    unix.close_fd(clock.fd)?
  }

  Ok()
}

pure module_name(method: Str) -> Str {
  match method {
    "udp-fixed" => "udp"
    "udp" => "default"
    _ => method
  }
}

pure method_help(method: Str) -> Str {
  match method {
    "icmp" => "  raw    use a raw ICMP socket only\n  dgram  use a datagram ICMP socket only\nOnly one of these may be specified: raw | dgram"
    "tcp" => "  syn    send a SYN (the default and only flag set)\n  reuse  allow reusing the local port (SO_REUSEADDR)"
    _ => f"No options for module `{module_name(method)}'"
  }
}

# Picks the ICMP socket type, opens the raw socket an ICMP or TCP trace needs,
# and runs the probes.
proc trace(s: Settings, tcp_raw_fd: Int) [process, env, io, time, net, error] -> Result[Unit] {
  let c = linux.net_constants()
  var raw_fd = tcp_raw_fd
  var icmp_mode = ""

  if s.method == "icmp" {
    icmp_mode = pick_icmp_sockets(s)?

    if icmp_mode == "raw" {
      raw_fd = open_socket(s, c.SOCK_RAW, if s.v6 { c.IPPROTO_ICMPV6 } else { c.IPPROTO_ICMP }, false)?
    }
  }

  run_trace(s, raw_fd, icmp_mode)
}

proc main(...argv: List[Str]) [process, env, io, time, net, error] {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {
        prog: "traceroute",
        status: 2,
        unsupported: {
          "-A": "AS path lookups need a routing-registry service",
          "--as-path-lookups": "AS path lookups need a routing-registry service",
          "-e": "ICMP extensions are not available from the error queue",
          "--extensions": "ICMP extensions are not available from the error queue",
          "-l": "IPv6 flow labels need the flow-label manager socket option",
          "--flowlabel": "IPv6 flow labels need the flow-label manager socket option",
          "-D": "DCCP probes need a DCCP socket",
          "--dccp": "DCCP probes need a DCCP socket",
          "-P": "raw protocol probes are not available",
          "--protocol": "raw protocol probes are not available",
          "--mtu": "path MTU discovery is not available",
          "--back": "backward-path hop guessing is not available",
        },
      },
      ipv4: {form: "-4", default: false, conflicts: ["ipv6"]},
      ipv6: {form: "-6", default: false, conflicts: ["ipv4"]},
      debug: {form: "-d --debug", default: false},
      dont_fragment: {form: "-F --dont-fragment", default: false},
      first: {form: "-f --first TTL"},
      gateway: {form: "-g --gateway GATES", repeated: true},
      icmp: {form: "-I --icmp", default: false, conflicts: ["tcp", "udp"]},
      tcp: {form: "-T --tcp", default: false, conflicts: ["icmp", "udp"]},
      udp: {form: "-U --udp", default: false, conflicts: ["icmp", "tcp"]},
      interface: {form: "-i --interface NAME"},
      max_hops: {form: "-m --max-hops TTL"},
      sim_queries: {form: "-N --sim-queries N"},
      numeric: {form: "-n", default: false},
      port: {form: "-p --port PORT"},
      tos: {form: "-t --tos TOS"},
      wait: {form: "-w --wait SPEC"},
      queries: {form: "-q --queries N"},
      bypass: {form: "-r", default: false},
      source: {form: "-s --source ADDR"},
      sendwait: {form: "-z --sendwait TIME"},
      module: {form: "-M --module NAME"},
      module_options: {form: "-O --options OPTS"},
      sport: {form: "--sport PORT"},
      fwmark: {form: "--fwmark MARK"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "-V --version", default: false, stop: true},
      operands: {form: "...HOST"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("traceroute")
    return
  }

  let argc = argv.len()

  if opts.operands.is_empty() {
    usage_die("Specify \"host\" missing argument.")
  }

  if opts.operands.len() > 2 {
    usage_die(f"Extra arg `{opts.operands[2]}' (position 3, argc {argc})")
  }

  var method = "udp"

  if let name = opts.module {
    method = match name {
      "default" => "udp"
      "udp" => "udp-fixed"
      "icmp" => "icmp"
      "tcp" => "tcp"
      "udplite" | "dccp" | "raw" => ""
      _ => "?"
    }

    if method == "" {
      usage_die(f"traceroute: module `{name}' is not supported")
    }

    if method == "?" {
      usage_die(f"Unknown traceroute module {name}")
    }
  }

  if opts.icmp {
    method = "icmp"
  } else if opts.tcp {
    method = "tcp"
  } else if opts.udp {
    method = "udp-fixed"
  }

  var icmp_sockets = "any"
  var module_help = false

  if let given = opts.module_options {
    for option in given.split(",") {
      if option == "help" {
        module_help = true
      } else if method == "icmp" and (option == "raw" or option == "dgram") {
        icmp_sockets = option
      } else if method == "tcp" and (option == "syn" or option == "reuse") {
        continue
      } else {
        usage_die(f"Unknown option `{option}' for module `{module_name(method)}'")
      }
    }
  }

  if module_help {
    gnu.write_text(f"{method_help(method)}\n")
    return
  }

  var first_ttl = 1
  var max_ttl = 30

  if let text = opts.max_hops {
    let value = text.parse_int_decimal() ?? -1

    if value < 0 and text.parse_int_decimal() is Err(_) {
      usage_die(f"Cannot handle `-m' option with arg `{text}' (argc {argc})")
    }

    if value < 0 or value > 255 {
      usage_die("max hops cannot be more than 255")
    }

    max_ttl = value
  }

  if let text = opts.first {
    let value = text.parse_int_decimal() ?? -1

    if text.parse_int_decimal() is Err(_) {
      usage_die(f"Cannot handle `-f' option with arg `{text}' (argc {argc})")
    }

    first_ttl = value
  }

  if first_ttl < 1 or first_ttl > 255 or first_ttl > max_ttl {
    usage_die("first hop out of range")
  }

  var queries = 3

  if let text = opts.queries {
    let value = text.parse_int_decimal() ?? -1

    if text.parse_int_decimal() is Err(_) {
      usage_die(f"Cannot handle `-q' option with arg `{text}' (argc {argc})")
    }

    if value < 1 or value > MAX_PROBES_PER_HOP {
      usage_die("no more than 10 probes per hop")
    }

    queries = value
  }

  if let text = opts.sim_queries {
    if bounded(text, 0, 1024) == null {
      usage_die(f"Cannot handle `-N' option with arg `{text}' (argc {argc})")
    }
  }

  var max_wait_ms = 5000
  var here = 3.0
  var near = 10.0

  if let text = opts.wait {
    let parts = text.split(",")

    if parts.len() > 3 {
      usage_die(f"Cannot handle `-w' option with arg `{text}' (argc {argc})")
    }

    var numbers: List[Float] = []

    for part in parts {
      guard let number = part.parse_float() else { |failure|
        usage_die(f"Cannot handle `-w' option with arg `{text}' (argc {argc})")
        return
      }

      if number < 0.0 {
        usage_die(f"bad wait specifications `{text}' used")
      }

      numbers += [number]
    }

    max_wait_ms = seconds_ms(parts[0]) ?? max_wait_ms

    if numbers.len() > 1 {
      here = numbers[1]
    }

    if numbers.len() > 2 {
      near = numbers[2]
    }
  }

  var send_wait_ms = 0

  if let text = opts.sendwait {
    let value = text.parse_float() ?? -1.0

    if text.parse_float() is Err(_) {
      usage_die(f"Cannot handle `-z' option with arg `{text}' (argc {argc})")
    }

    if value < 0.0 {
      usage_die(f"bad sendtime `{text}' specified")
    }

    send_wait_ms = if value > 10.0 { value.round() ?? 0 } else { (value * 1000.0).round() ?? 0 }
  }

  var port = match method {
    "icmp" => 1
    "tcp" => FIXED_TCP_PORT
    "udp-fixed" => FIXED_UDP_PORT
    _ => FIRST_UDP_PORT
  }

  if let text = opts.port {
    let value = bounded(text, 0, 65535)

    if value == null {
      usage_die(f"Cannot handle `-p' option with arg `{text}' (argc {argc})")
    }

    port = value ?? port
  }

  var tos: Int? = null

  if let text = opts.tos {
    tos = bounded(text, 0, 255)

    if tos == null {
      usage_die(f"Cannot handle `-t' option with arg `{text}' (argc {argc})")
    }
  }

  var source_port = 0

  if let text = opts.sport {
    let value = bounded(text, 1, 65535)

    if value == null {
      usage_die(f"Cannot handle `--sport' option with arg `{text}' (argc {argc})")
    }

    source_port = value ?? 0
  }

  var fwmark: Int? = null

  if let text = opts.fwmark {
    fwmark = bounded(text, 0, 4294967295)

    if fwmark == null {
      usage_die(f"Cannot handle `--fwmark' option with arg `{text}' (argc {argc})")
    }
  }

  let family = if opts.ipv4 { "ipv4" } else if opts.ipv6 { "ipv6" } else { "any" }
  let host = opts.operands[0]
  let target = resolve_host(host, family, argc)?

  var source: Str? = null

  if let text = opts.source {
    let found = dns.resolve_host(text, "any") ?? []

    if found.is_empty() {
      eprint f"{text}: Name or service not known"
      usage_die(f"Cannot handle `-s' option with arg `{text}' (argc {argc})")
    }

    let found_v6 = found[0].family == "ipv6"

    if found_v6 != target.v6 {
      usage_die("IP version mismatch in addresses specified")
    }

    source = found[0].addr
  }

  var gateways: List[Bytes] = []

  if ! opts.gateway.is_empty() and target.v6 {
    usage_die("traceroute: option '-g' is not supported: IPv6 routing headers are not available")
  }

  for list in opts.gateway {
    for name in list.split(",") {
      guard let gate = dotted_quad(name) else { |failure|
        eprint f"{name}: Name or service not known"
        usage_die(f"Cannot handle `-g' option with arg `{list}' (argc {argc})")
        return
      }

      gateways += [gate]
    }
  }

  if gateways.len() > 8 {
    usage_die("Too many gateways specified (maximum 8 for IPv4)")
  }

  # Header lengths for the printed packet size: IP header, IP options (the
  # route option plus the destination the kernel appends), and the transport
  # header. A TCP probe is a bare SYN, so its length cannot be chosen.
  let ip_header = if target.v6 { 40 } else { 20 }
  let options = if gateways.is_empty() { 0 } else { 8 + 4 * gateways.len() }
  let transport = if method == "tcp" { 20 } else { 8 }
  let base = ip_header + options + transport
  var packet_len = ip_header + options + 40
  var payload_len = packet_len - base

  if opts.operands.len() == 2 {
    let text = opts.operands[1]
    let value = text.parse_int_decimal() ?? -1

    if text.parse_int_decimal() is Err(_) {
      usage_die(f"Cannot handle \"packetlen\" cmdline arg `{text}' on position 2 (argc {argc})")
    }

    if value > 65535 {
      usage_die(f"too big packetlen {value} specified")
    }

    if method == "tcp" {
      usage_die("traceroute: PACKETLEN is not supported with -T: the probe is a bare SYN")
    }

    packet_len = if value < base { base } else { value }
    payload_len = packet_len - base
  }

  let s: Settings = {
    host: host,
    target: target.addr,
    v6: target.v6,
    method: method,
    numeric: opts.numeric,
    first_ttl: first_ttl,
    max_ttl: max_ttl,
    queries: queries,
    max_wait_ms: max_wait_ms,
    here: here,
    near: near,
    send_wait_ms: send_wait_ms,
    port: port,
    source: source,
    interface: opts.interface,
    tos: tos,
    dont_fragment: opts.dont_fragment,
    bypass: opts.bypass,
    debug: opts.debug,
    fwmark: fwmark,
    source_port: source_port,
    gateways: gateways,
    icmp_sockets: icmp_sockets,
    payload_len: payload_len,
    packet_len: packet_len,
  }

  let c = linux.net_constants()
  let protocol = if s.v6 { c.IPPROTO_ICMPV6 } else { c.IPPROTO_ICMP }
  let family_number = if s.v6 { c.AF_INET6 } else { c.AF_INET }
  var raw_fd = -1

  if method == "tcp" {
    match linux.socket(family_number, c.SOCK_RAW, protocol) {
      Ok(fd) => { raw_fd = fd }
      Err(failure) => {
        eprint "You do not have enough privileges to use this traceroute method."
        eprint f"socket: {gnu.strerror(failure)}"
        exit 1
      }
    }
  }

  gnu.write_text(f"traceroute to {s.host} ({s.target}), {s.max_ttl} hops max, {s.packet_len} byte packets")

  if let Err(failure) = trace(s, raw_fd) {
    eprint f"\n{failure.message}"
    exit 1
  }
}
