##! Socket-table collection and decoding over NETLINK_SOCK_DIAG.
##!
##! `collect_inet` and `collect_unix` send one dump request per address family
##! and decode every reply into a `DiagSocket`; the kernel decides the order
##! (listening hash first, then established, per family). The renderers in
##! this module (`endpoint_address`, `skmem_text`, `tcp_info_text`,
##! `timer_text`) reproduce the iproute2 `ss` texts so an applet and a test can
##! pin them against the reference tool.
##!
##! Socket-to-process ownership comes from `linux.open_files`, which is best
##! effort: a process whose descriptors cannot be read (another user's, without
##! privilege) contributes nothing, exactly as for the reference tool.

## Address family number of IPv4 sockets.
export const AF_INET = 2
## Address family number of unix-domain sockets.
export const AF_UNIX = 1
## Address family number of IPv6 sockets.
export const AF_INET6 = 10
const IPPROTO_TCP = 6
const IPPROTO_UDP = 17
const IPPROTO_RAW = 255
# inet_diag attribute numbers, which net_constants does not name.
const INET_DIAG_INFO = 2
const INET_DIAG_CONG = 4
const INET_DIAG_SKMEMINFO = 7
const INET_DIAG_SHUTDOWN = 8
const INET_DIAG_PROTOCOL = 10
const INET_DIAG_SKV6ONLY = 11
const INET_DIAG_MARK = 15
const INET_DIAG_CGROUP_ID = 21
# unix_diag attribute numbers and the unix_diag_req `show` bits.
const UNIX_DIAG_NAME = 0
const UNIX_DIAG_VFS = 1
const UNIX_DIAG_PEER = 2
const UNIX_DIAG_ICONS = 3
const UNIX_DIAG_RQLEN = 4
const UNIX_DIAG_MEMINFO = 5
const UNIX_DIAG_SHUTDOWN = 6
const UDIAG_SHOW_NAME = 1
const UDIAG_SHOW_PEER = 2
const UDIAG_SHOW_VFS = 4
const UDIAG_SHOW_ICONS = 8
const UDIAG_SHOW_RQLEN = 16
const UDIAG_SHOW_MEMINFO = 32
const HEX = "0123456789abcdef"
## Kernel state of an established connection.
export const TCP_ESTABLISHED = 1
## Kernel state of a connection that sent its SYN.
export const TCP_SYN_SENT = 2
## Kernel state of a connection that received a SYN.
export const TCP_SYN_RECV = 3
## Kernel state after the local side sent its FIN.
export const TCP_FIN_WAIT1 = 4
## Kernel state after the peer acknowledged the local FIN.
export const TCP_FIN_WAIT2 = 5
## Kernel state of a connection kept only to absorb late segments.
export const TCP_TIME_WAIT = 6
## Kernel state of an unconnected datagram, raw or unix socket and of a closed TCP one.
export const TCP_CLOSE = 7
## Kernel state after the peer's FIN arrived.
export const TCP_CLOSE_WAIT = 8
## Kernel state waiting for the last acknowledgement.
export const TCP_LAST_ACK = 9
## Kernel state of a listening socket.
export const TCP_LISTEN = 10
## Kernel state of a simultaneous close.
export const TCP_CLOSING = 11
const STATE_NAMES = ["UNKNOWN", "ESTAB", "SYN-SENT", "SYN-RECV", "FIN-WAIT-1", "FIN-WAIT-2", "TIME-WAIT", "UNCONN", "CLOSE-WAIT", "LAST-ACK", "LISTEN", "CLOSING"]

## The filesystem identity of a bound unix socket path.
export type VfsIdentity = {inode: Int, device: Int}

## One socket as the kernel reports it. `netid` names the table the reference
## tool prints (`tcp`, `udp`, `raw`, `u_str`, `u_dgr`, `u_seq`). For an inet
## socket `local_port`/`remote_port` are ports and `local`/`remote` are 4- or
## 16-byte addresses; for a unix socket they are the socket and peer inode
## numbers and `name` is the bound path (`@`-prefixed when abstract).
export type DiagSocket = {
  netid: Str,
  family: Int,
  protocol: Int,
  state: Int,
  timer: Int,
  retrans: Int,
  expires: Int,
  recv_queue: Int,
  send_queue: Int,
  local: Bytes,
  remote: Bytes,
  local_port: Int,
  remote_port: Int,
  ifindex: Int,
  uid: Int,
  inode: Int,
  cookie: Str,
  name: Str?,
  memory: Bytes?,
  info: Bytes?,
  congestion: Str?,
  shutdown: Int?,
  v6only: Int?,
  mark: Int?,
  cgroup_id: Int?,
  vfs: VfsIdentity?,
  pending: List[Int],
  detailed: Bool,
}

## A process that holds a socket descriptor.
export type Owner = {inode: Int, name: Str, pid: Int, fd: Int}

## Lowercase hexadecimal text of a non-negative integer.
export pure hex(value: Int) -> Str {
  if value == 0 { return "0" }
  var rest = value
  var out = ""
  while rest > 0 {
    let digit = rest % 16
    out = HEX.byte_slice(digit, length: 1) + out
    rest = rest / 16
  }
  out
}

pure u32(data: Bytes, offset: Int) -> Int {
  if offset + 4 > data.len() { return 0 }
  bytes.unpack_le(data, 4, offset) ?? 0
}

pure u64(data: Bytes, offset: Int) -> Int {
  if offset + 8 > data.len() { return 0 }
  bytes.unpack_le(data, 8, offset) ?? 0
}

pure u16be(data: Bytes, offset: Int) -> Int {
  bytes.unpack_be(data, 2, offset) ?? 0
}

type Attribute = {kind: Int, data: Bytes}

# Splits the rtattr list that follows a fixed header of `start` bytes. The
# type field's nested and byte-order flag bits are masked off.
pure attributes(payload: Bytes, start: Int) -> List[Attribute] {
  var found: List[Attribute] = []
  var offset = start
  while offset + 4 <= payload.len() {
    let length = bytes.unpack_le(payload, 2, offset) ?? 0
    let kind = (bytes.unpack_le(payload, 2, offset + 2) ?? 0).bit_and(16383)
    if length < 4 or offset + length > payload.len() { break }
    found += [{kind: kind, data: payload.slice(offset + 4, length - 4)}]
    offset += (length + 3) / 4 * 4
  }
  found
}

pure find_attribute(list: List[Attribute], kind: Int) -> Bytes? {
  for item in list {
    if item.kind == kind { return item.data }
  }
  null
}

pure cstring(data: Bytes) -> Str {
  var end = data.len()
  for index in range(data.len()) {
    if data.byte_at(index) == 0 {
      end = index
      break
    }
  }
  data.slice(0, end).utf8() ?? ""
}

pure cookie_text(low: Int, high: Int) -> Str {
  if high == 0 { return hex(low) }
  let tail = hex(low)
  hex(high) + "00000000".byte_slice(0, length: 8 - tail.byte_len()) + tail
}

## The column text the reference tool prints for a kernel state number.
export pure state_name(state: Int) -> Str {
  if state < 0 or state >= STATE_NAMES.len() { return "UNKNOWN" }
  STATE_NAMES[state]
}

pure inet_request(family: Int, protocol: Int, extensions: Int, states: Int) -> Result[Bytes] {
  # struct inet_diag_req_v2: family, protocol, ext, pad, states, then a
  # struct inet_diag_sockid whose cookie of all ones means "any socket".
  bytes.concat(
    [
      bytes.from_ints([family, protocol, extensions, 0])?,
      bytes.pack_le(states, 4)?,
      bytes.zero(40)?,
      bytes.from_ints([255, 255, 255, 255, 255, 255, 255, 255])?,
    ],
  )
}

pure decode_inet(payload: Bytes, protocol: Int) -> DiagSocket? {
  return null when payload.len() < 72

  let family = payload.byte_at(0) ?? 0
  let size = if family == AF_INET6 { 16 } else { 4 }
  let list = attributes(payload, 72)
  let reported = find_attribute(list, INET_DIAG_PROTOCOL)
  let real_protocol = if let data = reported { data.byte_at(0) ?? protocol } else { protocol }
  let netid = if real_protocol == IPPROTO_UDP { "udp" } else if real_protocol == IPPROTO_TCP { "tcp" } else { "raw" }
  var congestion: Str? = null
  if let data = find_attribute(list, INET_DIAG_CONG) { congestion = cstring(data) }
  var mark: Int? = null
  if let data = find_attribute(list, INET_DIAG_MARK) { mark = u32(data, 0) }
  var cgroup_id: Int? = null
  if let data = find_attribute(list, INET_DIAG_CGROUP_ID) { cgroup_id = u64(data, 0) }
  var shutdown: Int? = null
  if let data = find_attribute(list, INET_DIAG_SHUTDOWN) { shutdown = data.byte_at(0) }
  var v6only: Int? = null
  if let data = find_attribute(list, INET_DIAG_SKV6ONLY) { v6only = data.byte_at(0) }
  {
    netid: netid,
    family: family,
    protocol: real_protocol,
    state: payload.byte_at(1) ?? 0,
    timer: payload.byte_at(2) ?? 0,
    retrans: payload.byte_at(3) ?? 0,
    expires: u32(payload, 52),
    recv_queue: u32(payload, 56),
    send_queue: u32(payload, 60),
    local: payload.slice(8, size),
    remote: payload.slice(24, size),
    local_port: u16be(payload, 4),
    remote_port: u16be(payload, 6),
    ifindex: u32(payload, 40),
    uid: u32(payload, 64),
    inode: u32(payload, 68),
    cookie: cookie_text(u32(payload, 44), u32(payload, 48)),
    name: null,
    memory: find_attribute(list, INET_DIAG_SKMEMINFO),
    info: find_attribute(list, INET_DIAG_INFO),
    congestion: congestion,
    shutdown: shutdown,
    v6only: v6only,
    mark: mark,
    cgroup_id: cgroup_id,
    vfs: null,
    pending: [],
    detailed: true,
  }
}

pure decode_unix(payload: Bytes) -> DiagSocket? {
  return null when payload.len() < 16

  let kind = payload.byte_at(1) ?? 0
  let netid = if kind == 1 { "u_str" } else if kind == 2 { "u_dgr" } else { "u_seq" }
  let list = attributes(payload, 16)
  var name: Str? = null
  if let data = find_attribute(list, UNIX_DIAG_NAME) {
    if data.byte_at(0) == 0 {
      # An abstract name begins with a NUL byte and may hold more of them;
      # the reference tool shows every NUL as '@'.
      var text = ""
      for index in range(data.len()) {
        let byte = data.byte_at(index) ?? 0
        text += if byte == 0 { "@" } else { data.slice(index, 1).utf8() ?? "?" }
      }
      name = text
    } else {
      name = cstring(data)
    }
  }
  var recv = 0
  var send = 0
  if let data = find_attribute(list, UNIX_DIAG_RQLEN) {
    recv = u32(data, 0)
    send = u32(data, 4)
  }
  var peer = 0
  if let data = find_attribute(list, UNIX_DIAG_PEER) { peer = u32(data, 0) }
  var vfs: VfsIdentity? = null
  if let data = find_attribute(list, UNIX_DIAG_VFS) { vfs = {inode: u32(data, 0), device: u32(data, 4)} }
  var pending: List[Int] = []
  if let data = find_attribute(list, UNIX_DIAG_ICONS) {
    for index in range(data.len() / 4) { pending += [u32(data, index * 4)] }
  }
  var shutdown: Int? = null
  if let data = find_attribute(list, UNIX_DIAG_SHUTDOWN) { shutdown = data.byte_at(0) }
  {
    netid: netid,
    family: AF_UNIX,
    protocol: 0,
    state: payload.byte_at(2) ?? 0,
    timer: 0,
    retrans: 0,
    expires: 0,
    recv_queue: recv,
    send_queue: send,
    local: b"",
    remote: b"",
    local_port: u32(payload, 4),
    remote_port: peer,
    ifindex: 0,
    uid: 0,
    inode: u32(payload, 4),
    cookie: cookie_text(u32(payload, 8), u32(payload, 12)),
    name: name,
    memory: find_attribute(list, UNIX_DIAG_MEMINFO),
    info: null,
    congestion: null,
    shutdown: shutdown,
    v6only: null,
    mark: null,
    cgroup_id: null,
    vfs: vfs,
    pending: pending,
    detailed: true,
  }
}

proc dump(nl: Int, payload: Bytes) [process, error] -> Result[List[LinuxNetlinkMessage]] {
  let c = linux.net_constants()
  let replies = linux.netlink_request(nl, c.SOCK_DIAG_BY_FAMILY, c.NLM_F_REQUEST.bit_or(c.NLM_F_DUMP), payload)?
  Ok([item for item in replies if item.type == c.SOCK_DIAG_BY_FAMILY])
}

## Dumps the inet sockets of one family and transport protocol whose kernel
## state is in the `states` bitmask. `memory` and `tcp_info` request the
## SKMEMINFO and INET_DIAG_INFO/CONG attributes. The netlink primitive reports
## no error when the kernel lacks the diag module for a protocol (the dump just
## ends), so an empty list does not prove there are no sockets.
export proc collect_inet(nl: Int, family: Int, protocol: Int, states: Int, memory: Bool, tcp_info: Bool) [process, error] -> Result[List[DiagSocket], Error] {
  # idiag_ext is a bitmask of (attribute - 1): SHUTDOWN is always wanted so
  # the extended view can show it; MEMINFO and SKMEMINFO follow -m; INFO,
  # VEGASINFO and CONG follow -i.
  let extensions = 128 + (if memory { 1 + 64 } else { 0 }) + (if tcp_info { 2 + 4 + 8 } else { 0 })
  let request = inet_request(family, protocol, extensions, states)?
  let replies = dump(nl, request)?
  var found: List[DiagSocket] = []
  for reply in replies {
    if let item = decode_inet(reply.payload, protocol) { found += [item] }
  }
  Ok(found)
}

## Dumps the unix sockets whose kernel state is in `states`. `details` adds the
## filesystem identity and pending-connection lists the extended view shows.
export proc collect_unix(nl: Int, states: Int, memory: Bool, details: Bool) [process, error] -> Result[List[DiagSocket], Error] {
  var show = UDIAG_SHOW_NAME + UDIAG_SHOW_PEER + UDIAG_SHOW_RQLEN
  if memory { show += UDIAG_SHOW_MEMINFO }
  if details { show += UDIAG_SHOW_VFS + UDIAG_SHOW_ICONS }
  # struct unix_diag_req: family, protocol, pad, states, inode, show, cookie[2]
  let request = bytes.concat(
    [
      bytes.from_ints([AF_UNIX, 0, 0, 0])?,
      bytes.pack_le(states, 4)?,
      bytes.pack_le(0, 4)?,
      bytes.pack_le(show, 4)?,
      bytes.from_ints([255, 255, 255, 255, 255, 255, 255, 255])?,
    ],
  )
  let replies = dump(nl, request)?
  var found: List[DiagSocket] = []
  for reply in replies {
    if let item = decode_unix(reply.payload) { found += [item] }
  }
  Ok(found)
}

# Decodes the hex of a /proc/net address: each 32-bit word is printed in
# host (little-endian) order.
pure proc_address(text: Str, size: Int) -> Bytes {
  var raw: List[Int] = []
  for word in range(size / 4) {
    let chunk = text.byte_slice(word * 8, length: 8)
    for index in range(4) {
      let at = (3 - index) * 2
      raw += [f"0x{chunk.byte_slice(at, length: 2)}".parse_int() ?? 0]
    }
  }
  bytes.from_ints(raw) ?? b""
}

## Raw sockets from /proc/net/raw or raw6, for kernels without the raw_diag
## module (sock_diag then answers with nothing instead of an error). The
## reference tool falls back to the same files. The rows carry no memory,
## shutdown or cgroup attributes, so `detailed` is false and callers print no
## extended text for them.
export proc collect_raw_proc(family: Int, states: Int) [fs, error] -> Result[List[DiagSocket], Error] {
  var found: List[DiagSocket] = []
  let source = if family == AF_INET6 { fp"/proc/net/raw6" } else { fp"/proc/net/raw" }
  let size = if family == AF_INET6 { 16 } else { 4 }
  let content = source.read_text() ?? ""
  var first = true
  for line in content.lines() {
    if first {
      first = false
      continue
    }
    let fields = line.fields()
    if fields.len() < 10 { continue }
    let local = fields[1].split(":")
    let remote = fields[2].split(":")
    let queues = fields[4].split(":")
    if local.len() != 2 or remote.len() != 2 or queues.len() != 2 { continue }
    let state = f"0x{fields[3]}".parse_int() ?? 0
    let bit = if state >= 0 and state < 12 { [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048][state] } else { 0 }
    if states / bit % 2 != 1 { continue }
    found += [{
      netid: "raw", family: family, protocol: 0, state: state, timer: 0, retrans: 0, expires: 0,
      recv_queue: f"0x{queues[1]}".parse_int() ?? 0, send_queue: f"0x{queues[0]}".parse_int() ?? 0,
      local: proc_address(local[0], size), remote: proc_address(remote[0], size),
      local_port: f"0x{local[1]}".parse_int() ?? 0, remote_port: f"0x{remote[1]}".parse_int() ?? 0,
      ifindex: 0, uid: fields[7].parse_int() ?? 0, inode: fields[9].parse_int() ?? 0, cookie: "0",
      name: null, memory: null, info: null, congestion: null, shutdown: null, v6only: null,
      mark: null, cgroup_id: null, vfs: null, pending: [], detailed: false,
    }]
  }
  Ok(found)
}

## Opens the sock_diag netlink channel the collectors share.
export proc open() [process, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  linux.netlink_open(c.NETLINK_SOCK_DIAG)
}

## The transport protocol numbers `collect_inet` accepts for each table.
export pure protocol_number(netid: Str) -> Int {
  match netid {
    "tcp" => IPPROTO_TCP
    "udp" => IPPROTO_UDP
    else => IPPROTO_RAW
  }
}

## Every descriptor that refers to a socket, in the order the reference tool
## prints owners: the last one found for an inode comes first.
export proc owners() [process, error] -> Result[List[Owner], Error] {
  var found: List[Owner] = []
  let table: List[ProcessEntry] = process.list()? |> sort-by .pid
  for entry in table {
    let files = linux.open_files(entry.pid)
    if let Ok(list) = files {
      for file in list {
        if file.path.display().starts_with("socket:[") {
          found += [{inode: file.inode, name: entry.command, pid: entry.pid, fd: file.fd}]
        }
      }
    }
  }
  Ok(found)
}

## `users:(("name",pid=N,fd=N),...)` text for one inode, or null.
export pure owners_text(list: List[Owner], inode: Int) -> Str? {
  if inode == 0 { return null }
  var parts: List[Str] = []
  for item in list {
    if item.inode == inode { parts = [f"(\"{item.name}\",pid={item.pid},fd={item.fd})"].extend(parts) }
  }
  return null when parts.is_empty()

  f"users:({parts.join(",")})"
}

## Dotted-quad text of four bytes.
export pure ipv4_text(data: Bytes) -> Str {
  f"{data.byte_at(0) ?? 0}.{data.byte_at(1) ?? 0}.{data.byte_at(2) ?? 0}.{data.byte_at(3) ?? 0}"
}

## Compressed IPv6 text of sixteen bytes; a v4-mapped address keeps the
## dotted tail.
export pure ipv6_text(data: Bytes) -> Str {
  var mapped = true
  for index in range(10) {
    if data.byte_at(index) != 0 { mapped = false }
  }
  if mapped and data.byte_at(10) == 255 and data.byte_at(11) == 255 {
    return f"::ffff:{ipv4_text(data.slice(12, 4))}"
  }

  var groups: List[Int] = []
  for index in range(8) {
    groups += [(data.byte_at(index * 2) ?? 0) * 256 + (data.byte_at(index * 2 + 1) ?? 0)]
  }
  # The first longest run of two or more zero groups becomes "::".
  var best_start = -1
  var best_length = 1
  var index = 0
  while index < 8 {
    if groups[index] == 0 {
      var end = index
      while end < 8 and groups[end] == 0 { end += 1 }
      if end - index > best_length {
        best_start = index
        best_length = end - index
      }
      index = end
    } else {
      index += 1
    }
  }
  var out = ""
  var position = 0
  while position < 8 {
    if position == best_start {
      out += "::"
      position += best_length
    } else {
      if out != "" and !out.ends_with(":") { out += ":" }
      out += hex(groups[position])
      position += 1
    }
  }
  out
}

## The address text of an inet socket endpoint: `*` for the unspecified IPv6
## address of a dual-stack socket, otherwise the numeric text.
export pure endpoint_address(data: Bytes, family: Int, v6only: Bool) -> Str {
  if family == AF_INET { return ipv4_text(data) }

  var zero = true
  for index in range(16) {
    if data.byte_at(index) != 0 { zero = false }
  }
  if zero and !v6only { return "*" }

  ipv6_text(data)
}

pure format_g(microseconds: Int) -> Str {
  # printf("%g") of microseconds / 1000: six significant digits, no trailing
  # zeros. The value is a whole number of microseconds so the digits are
  # exact up to 999 ms and rounded half up beyond.
  let whole = microseconds / 1000
  var fraction = microseconds % 1000
  var digits = f"{whole}".byte_len()
  if whole == 0 { digits = 1 }
  var keep = 6 - digits
  if keep < 0 { keep = 0 }
  if keep > 3 { keep = 3 }
  var scaled = fraction
  var shown = whole
  if keep < 3 {
    var divisor = 1
    for _ in range(3 - keep) { divisor *= 10 }
    scaled = (fraction + divisor / 2) / divisor
    if scaled * divisor >= 1000 {
      shown += 1
      scaled = 0
    }
    fraction = scaled * divisor
  }
  if fraction == 0 { return f"{shown}" }

  var text = f"{fraction}"
  while text.byte_len() < 3 { text = "0" + text }
  while text.ends_with("0") { text = text.byte_slice(0, length: text.byte_len() - 1) }
  f"{shown}.{text}"
}

pure round_div(numerator: Int, denominator: Int) -> Int {
  # Round half to even, as printf does for an exactly representable tie.
  let quotient = numerator / denominator
  let remainder = numerator % denominator
  let twice = remainder * 2
  if twice > denominator { return quotient + 1 }
  if twice == denominator and quotient % 2 == 1 { return quotient + 1 }
  quotient
}

## `skmem:(r..,rb..)` text from a SKMEMINFO or MEMINFO attribute.
export pure skmem_text(data: Bytes) -> Str {
  let names = ["r", "rb", "t", "tb", "f", "w", "o", "bl", "d"]
  var parts: List[Str] = []
  for index in range(names.len()) {
    if index * 4 + 4 > data.len() { break }
    parts += [f"{names[index]}{u32(data, index * 4)}"]
  }
  f"skmem:({parts.join(",")})"
}

pure print_ms(timeout: Int) -> Str {
  # print_ms_timer of the reference tool: minutes drop the lower units once
  # they are two digits, seconds drop milliseconds once they are two digits.
  var seconds = timeout / 1000
  let minutes = seconds / 60
  seconds = seconds % 60
  var millis = timeout % 1000
  var out = ""
  if minutes > 0 {
    millis = 0
    out += f"{minutes}min"
    if minutes > 9 { seconds = 0 }
  }
  if seconds > 0 {
    if seconds > 9 { millis = 0 }
    out += f"{seconds}" + (if millis > 0 { "." } else { "sec" })
  }
  if millis > 0 {
    out += (if millis < 10 { "00" } else if millis < 100 { "0" } else { "" }) + f"{millis}" + "ms"
  }
  out
}

## `timer:(kind,time,retransmits)` for an armed TCP timer, or null.
export pure timer_text(timer: Int, expires: Int, retrans: Int) -> Str? {
  return null when timer == 0

  let kind = match timer {
    1 => "on"
    2 => "keepalive"
    3 => "timewait"
    4 => "persist"
    else => "unknown"
  }
  f"timer:({kind},{print_ms(expires)},{retrans})"
}

## The `-i` text of a TCP socket: option flags (when `options` is set), the
## congestion algorithm, and the tcp_info counters in the reference tool's
## order. Zero counters are omitted. Algorithm-specific blocks (bbr, dctcp,
## vegas) are not decoded.
export pure tcp_info_text(info: Bytes, congestion: Str?, state: Int, options: Bool) -> Str {
  var out: List[Str] = []
  let flags = info.byte_at(5) ?? 0
  if options {
    if flags.bit_and(1) != 0 { out += ["ts"] }
    if flags.bit_and(2) != 0 { out += ["sack"] }
    if flags.bit_and(8) != 0 { out += ["ecn"] }
    if flags.bit_and(16) != 0 { out += ["ecnseen"] }
    if flags.bit_and(32) != 0 { out += ["fastopen"] }
  }
  if let name = congestion { if name != "" { out += [name] } }
  return out.join(" ") when info.is_empty()

  let wscale = info.byte_at(6) ?? 0
  let rto = u32(info, 8)
  let ato = u32(info, 12)
  let mss = u32(info, 16)
  let rcv_mss = u32(info, 20)
  let unacked = u32(info, 24)
  let sacked = u32(info, 28)
  let lost = u32(info, 32)
  let retrans = u32(info, 36)
  let fackets = u32(info, 40)
  let last_sent = u32(info, 44)
  let last_recv = u32(info, 52)
  let last_ack = u32(info, 56)
  let pmtu = u32(info, 60)
  let rcv_ssthresh = u32(info, 64)
  let rtt = u32(info, 68)
  let rttvar = u32(info, 72)
  let snd_ssthresh = u32(info, 76)
  let cwnd = u32(info, 80)
  let advmss = u32(info, 84)
  let reordering = u32(info, 88)
  let rcv_rtt = u32(info, 92)
  let rcv_space = u32(info, 96)
  let total_retrans = u32(info, 100)
  let pacing = u64(info, 104)
  let max_pacing = u64(info, 112)
  let bytes_acked = u64(info, 120)
  let bytes_received = u64(info, 128)
  let segs_out = u32(info, 136)
  let segs_in = u32(info, 140)
  let not_sent = u32(info, 144)
  let min_rtt = u32(info, 148)
  let data_segs_in = u32(info, 152)
  let data_segs_out = u32(info, 156)
  let delivery_rate = u64(info, 160)
  let busy = u64(info, 168)
  let rwnd_limited = u64(info, 176)
  let sndbuf_limited = u64(info, 184)
  let delivered = u32(info, 192)
  let delivered_ce = u32(info, 196)
  let bytes_sent = u64(info, 200)
  let bytes_retrans = u64(info, 208)
  let dsack_dups = u32(info, 216)
  let reord_seen = u32(info, 220)
  let rcv_ooopack = u32(info, 224)
  let snd_wnd = u32(info, 228)
  let rcv_wnd = u32(info, 232)
  let backoff = info.byte_at(4) ?? 0
  let app_limited = (info.byte_at(7) ?? 0).bit_and(1) != 0

  if flags.bit_and(4) != 0 { out += [f"wscale:{wscale.bit_and(15)},{wscale / 16}"] }
  if rto != 0 and rto != 3000000 { out += [f"rto:{format_g(rto)}"] }
  if backoff != 0 { out += [f"backoff:{backoff}"] }
  if rtt != 0 { out += [f"rtt:{format_g(rtt)}/{format_g(rttvar)}"] }
  if ato != 0 { out += [f"ato:{format_g(ato)}"] }
  if mss != 0 { out += [f"mss:{mss}"] }
  if pmtu != 0 { out += [f"pmtu:{pmtu}"] }
  if rcv_mss != 0 { out += [f"rcvmss:{rcv_mss}"] }
  if advmss != 0 { out += [f"advmss:{advmss}"] }
  if cwnd != 0 { out += [f"cwnd:{cwnd}"] }
  if snd_ssthresh != 0 and snd_ssthresh < 65535 { out += [f"ssthresh:{snd_ssthresh}"] }
  if bytes_sent != 0 { out += [f"bytes_sent:{bytes_sent}"] }
  if bytes_retrans != 0 { out += [f"bytes_retrans:{bytes_retrans}"] }
  if bytes_acked != 0 { out += [f"bytes_acked:{bytes_acked}"] }
  if bytes_received != 0 { out += [f"bytes_received:{bytes_received}"] }
  if segs_out != 0 { out += [f"segs_out:{segs_out}"] }
  if segs_in != 0 { out += [f"segs_in:{segs_in}"] }
  if data_segs_out != 0 { out += [f"data_segs_out:{data_segs_out}"] }
  if data_segs_in != 0 { out += [f"data_segs_in:{data_segs_in}"] }
  # send rate: cwnd * mss * 8 bits over the smoothed round trip, rounded.
  if rtt > 0 and mss > 0 and cwnd > 0 { out += [f"send {round_div(cwnd * mss * 8000000, rtt)}bps"] }
  if last_sent != 0 { out += [f"lastsnd:{last_sent}"] }
  if last_recv != 0 { out += [f"lastrcv:{last_recv}"] }
  if last_ack != 0 { out += [f"lastack:{last_ack}"] }
  if pacing != 0 and pacing != -1 {
    var text = f"pacing_rate {pacing * 8}bps"
    if max_pacing != 0 and max_pacing != -1 { text += f"/{max_pacing * 8}bps" }
    out += [text]
  }
  if delivery_rate != 0 { out += [f"delivery_rate {delivery_rate * 8}bps"] }
  if delivered != 0 { out += [f"delivered:{delivered}"] }
  if delivered_ce != 0 { out += [f"delivered_ce:{delivered_ce}"] }
  if app_limited { out += ["app_limited"] }
  if busy != 0 {
    out += [f"busy:{busy / 1000}ms"]
    if rwnd_limited != 0 { out += [f"rwnd_limited:{rwnd_limited / 1000}ms({percent(rwnd_limited, busy)}%)"] }
    if sndbuf_limited != 0 { out += [f"sndbuf_limited:{sndbuf_limited / 1000}ms({percent(sndbuf_limited, busy)}%)"] }
  }
  if unacked != 0 { out += [f"unacked:{unacked}"] }
  if retrans != 0 or total_retrans != 0 { out += [f"retrans:{retrans}/{total_retrans}"] }
  if lost != 0 { out += [f"lost:{lost}"] }
  if sacked != 0 and state != TCP_LISTEN { out += [f"sacked:{sacked}"] }
  if dsack_dups != 0 { out += [f"dsack_dups:{dsack_dups}"] }
  if fackets != 0 { out += [f"fackets:{fackets}"] }
  if reordering != 3 and reordering != 0 { out += [f"reordering:{reordering}"] }
  if reord_seen != 0 { out += [f"reord_seen:{reord_seen}"] }
  if rcv_rtt != 0 { out += [f"rcv_rtt:{format_g(rcv_rtt)}"] }
  if rcv_space != 0 { out += [f"rcv_space:{rcv_space}"] }
  if rcv_ssthresh != 0 { out += [f"rcv_ssthresh:{rcv_ssthresh}"] }
  if not_sent != 0 { out += [f"notsent:{not_sent}"] }
  if min_rtt != 0 { out += [f"minrtt:{format_g(min_rtt)}"] }
  if rcv_ooopack != 0 { out += [f"rcv_ooopack:{rcv_ooopack}"] }
  if snd_wnd != 0 { out += [f"snd_wnd:{snd_wnd}"] }
  if rcv_wnd != 0 { out += [f"rcv_wnd:{rcv_wnd}"] }
  out.join(" ")
}

pure percent(part: Int, whole: Int) -> Str {
  # One decimal place of 100 * part / whole, as "%.1f".
  let tenths = round_div(part * 1000, whole)
  f"{tenths / 10}.{tenths % 10}"
}
