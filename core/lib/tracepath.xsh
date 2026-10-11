##! tracepath: discover the hops and path MTU toward a destination with UDP
##! probes of increasing TTL, reading each hop's answer from the socket's
##! error queue, in the layout iputils prints.
use gnu
use icmp

const USAGE = """Usage
  tracepath [options] <destination>

Options:
  -4             use IPv4
  -6             use IPv6
  -b             print both name and IP
  -l <length>    use packet <length>
  -m <hops>      use maximum <hops>
  -n             no reverse DNS name resolution
  -p <port>      use destination <port>
  -V             print version and exit
  <destination>  DNS name or IP address

For more details see tracepath(8).
"""

type Options = {
  ipv4: Bool, ipv6: Bool, both: Bool, numeric: Bool, help: Bool, version: Bool,
  length: Str?, hops: Str?, port: Str?, destinations: List[Str]
}

# One probe in flight: the TTL it carried and when it left, keyed by the
# slot its destination port encodes.
type Sent = {hops: Int, at_ns: Int}

# What the walk has learned so far. `slot` is the next probe slot; `names`
# caches reverse lookups.
type Walk = {
  mtu: Int, hops_to: Int, hops_from: Int, history: Map[Int, Sent], slot: Int,
  names: Map[Str]
}

# The result of reading the error queue: `progress` is iputils' verdict, -1
# when nothing arrived, 0 when the walk is over, otherwise the current MTU.
type Reading = {walk: Walk, progress: Int}

type Setup = {
  v6: Bool, address: Str, base_port: Int, overhead: Int, numeric: Bool, both: Bool
}

const LONG_DECIMAL = rx"^[ \t]*[+-]?[0-9]+$"
const INT_MAX = 2147483647
const HOST_COLUMN = 52
const HISTORY_SLOTS = 64
const PROBE_HEADER = 24

# A strtol-style integer option inside LOW..=HIGH; a violation ends the
# program with status 1.
proc long_option(text: Str, low: Int, high: Int) [process, env] -> Int {
  guard LONG_DECIMAL.matches(text) else {
    gnu.error(f"invalid argument: '{text}'")
    exit 1
  }
  let trimmed = text.trim()
  let digits = if trimmed.starts_with("+") { trimmed.byte_slice(1) } else { trimmed }
  guard let value = digits.parse_int() else {
    gnu.error(f"invalid argument: '{text}': Numerical result out of range")
    exit 1
  }
  if value < low or value > high {
    gnu.error(f"invalid argument: '{text}': out of range: {low} <= value <= {high}")
    exit 1
  }
  value
}

# The text of the errno values a probe answer can carry.
pure errno_text(errno: Int) -> Str {
  match errno {
    1 => "Operation not permitted"
    13 => "Permission denied"
    22 => "Invalid argument"
    90 => "Message too long"
    100 => "Network is down"
    101 => "Network is unreachable"
    111 => "Connection refused"
    113 => "No route to host"
    else => f"Unknown error {errno}"
  }
}

pure max_int(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

pure min_int(left: Int, right: Int) -> Int {
  if left < right { left } else { right }
}

# The destination column: the name (and the address with -b), padded to the
# fixed column iputils uses, with at least one space after it.
pure host_column(name: Str, address: Str, both: Bool) -> Str {
  let text = if both { f"{name} ({address})" } else { name }
  let used = min_int(text.byte_len(), HOST_COLUMN - 1)
  text + [" " for _ in range(HOST_COLUMN - used)].join("")
}

proc host_name(address: Str) [net] -> Str {
  let found = dns.reverse(address)
  if let Ok(list) = found {
    if ! list.is_empty() { return list[0] }
  }
  address
}

# A zeroed probe whose first bytes carry the TTL and the send time, which a
# router quoting the datagram back returns to us.
pure probe_payload(size: Int, hops: Int, stamp_ms: Int) -> Result[Bytes, Error] {
  let header = bytes.concat([bytes.pack_le(hops, 4)?, bytes.zero(4)?, bytes.pack_le(stamp_ms / 1000, 8)?, bytes.pack_le(stamp_ms % 1000 * 1000, 8)?])
  if size <= PROBE_HEADER { return Ok(header.slice(0, size)) }
  Ok(bytes.concat([header, bytes.zero(size - PROBE_HEADER)?]))
}

# Read and print every queued error. Returns what the walk learned.
proc read_answers(fd: Int, ttl: Int, start: Walk, setup: Setup, clock: icmp.Clock) [net, process, env, time, io, error] -> Result[Reading, Error] {
  var walk = start
  var progress = -1
  loop {
    let queued = icmp.read_error(fd)?
    guard let entry = queued else { break }
    progress = walk.mtu
    let received_ns = entry.stamp_ns ?? icmp.clock_ns(clock)?
    let slot = entry.port - setup.base_port
    var sent_hops = -1
    var sent_ns: Int? = null
    if slot >= 0 and slot < HISTORY_SLOTS - 1 and slot in walk.history {
      let known = walk.history.get(slot)?
      sent_hops = known.hops
      sent_ns = known.at_ns
      walk = {...walk, history: walk.history.remove(slot)}
    }
    var broken_router = false
    if entry.payload.len() >= PROBE_HEADER {
      let carried_ttl = bytes.unpack_le(entry.payload, 4, 0)?
      let carried_sec = bytes.unpack_le(entry.payload, 8, 8)?
      if carried_ttl == 0 or carried_sec == 0 {
        broken_router = true
      } else {
        sent_hops = carried_ttl
        if sent_ns == null {
          sent_ns = carried_sec * 1000000000 + bytes.unpack_le(entry.payload, 8, 16)? * 1000
        }
      }
    }
    var line = ""
    if entry.origin == 1 {
      line = f"{ttl:>2}?: {"[LOCALHOST]":<32} "
    } else if entry.origin == 2 or entry.origin == 3 {
      line = if sent_hops > 0 { f"{sent_hops:>2}:  " } else { f"{ttl:>2}?: " }
      let name = entry.offender
      let lookup = ! setup.numeric or setup.both
      if lookup and name not in walk.names {
        walk = {...walk, names: walk.names.set(name, host_name(name))}
      }
      let shown = if lookup { walk.names.get(name) ?? name } else { name }
      line += host_column(shown, name, setup.both)
    }
    if let departed = sent_ns {
      let micros = max_int((received_ns - departed) / 1000, 0)
      line += f"{micros / 1000:>3}.{micros % 1000:03}ms "
      if broken_router { line += "(This broken router returned corrupted payload) " }
    }
    var echoed = entry.ttl ?? -1
    if echoed <= 64 {
      echoed = 65 - echoed
    } else if echoed <= 128 {
      echoed = 129 - echoed
    } else {
      echoed = 256 - echoed
    }
    match entry.errno {
      110 => {
        gnu.write_text(line + "\n")
      }
      90 => {
        gnu.write_text(line + f"pmtu {entry.info}\n")
        walk = {...walk, mtu: entry.info}
        progress = entry.info
      }
      111 => {
        gnu.write_text(line + "reached\n")
        walk = {...walk, hops_to: if sent_hops < 0 { ttl } else { sent_hops }, hops_from: echoed}
        return Ok({walk: walk, progress: 0})
      }
      71 => {
        gnu.write_text(line + "!P\n")
        return Ok({walk: walk, progress: 0})
      }
      113 => {
        let expired = (entry.origin == 2 and entry.kind == 11 and entry.code == 0) or (entry.origin == 3 and entry.kind == 3 and entry.code == 0)
        if ! expired {
          gnu.write_text(line + "!H\n")
          return Ok({walk: walk, progress: 0})
        }
        if echoed >= 0 {
          if sent_hops >= 0 and echoed != sent_hops {
            line += f"asymm {echoed:>2} "
          } else if sent_hops < 0 and echoed != ttl {
            line += f"asymm {echoed:>2} "
          }
        }
        gnu.write_text(line + "\n")
      }
      101 => {
        gnu.write_text(line + "!N\n")
        return Ok({walk: walk, progress: 0})
      }
      13 => {
        gnu.write_text(line + "!A\n")
        return Ok({walk: walk, progress: 0})
      }
      else => {
        gnu.write_text(line + "\n")
        eprint f"NET ERROR: {errno_text(entry.errno)}"
        return Ok({walk: walk, progress: 0})
      }
    }
  }
  Ok({walk: walk, progress: progress})
}


# One TTL step of the walk: send a probe, wait up to a second for the error
# queue to answer, and read every queued answer.
proc probe(fd: Int, ttl: Int, start: Walk, setup: Setup, clock: icmp.Clock) [net, process, env, time, io, error] -> Result[Reading, Error] {
  let c = linux.net_constants()
  var walk = start
  var failures = 0
  var restarts = 0
  var sent = false
  while ! sent and failures < 10 {
    let payload = probe_payload(walk.mtu - setup.overhead, ttl, time.now())?
    let marked = icmp.clock_mark(clock)?
    let result = linux.sendto(fd, payload, {family: if setup.v6 { "inet6" } else { "inet" }, address: setup.address, port: setup.base_port + walk.slot})
    let at_ns = icmp.clock_collect(clock, marked)?
    # The slot is recorded even when the send fails: the kernel's local
    # error for a probe that is too large is matched to it by port.
    walk = {...walk, history: walk.history.set(walk.slot, {hops: ttl, at_ns: at_ns})}
    if result is Ok(_) {
      sent = true
      continue
    }
    let reading = read_answers(fd, ttl, walk, setup, clock)?
    walk = {...reading.walk, history: reading.walk.history.remove(walk.slot)}
    if reading.progress == 0 { return Ok({walk: walk, progress: 0}) }
    if reading.progress > 0 and restarts < 10 {
      restarts += 1
      failures = 0
      continue
    }
    failures += 1
  }
  if ! sent {
    gnu.write_text(f"{ttl:>2}:  send failed\n")
    return Ok({walk: walk, progress: 0})
  }
  walk = {...walk, slot: (walk.slot + 1) % HISTORY_SLOTS}
  let _ = unix.poll_fd(fd, ["readable"], 1000)?
  if icmp.receive(fd, c.MSG_DONTWAIT) is Ok(_) {
    gnu.write_text(f"{ttl:>2}?: reply received 8)\n")
    return Ok({walk: walk, progress: 0})
  }
  read_answers(fd, ttl, walk, setup, clock)
}

proc open_probe_socket(v6: Bool) [process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let fd = linux.socket(if v6 { c.AF_INET6 } else { c.AF_INET }, c.SOCK_DGRAM)?
  let level = if v6 { c.SOL_IPV6 } else { c.SOL_IP }
  let discover = if v6 { c.IPV6_MTU_DISCOVER } else { c.IP_MTU_DISCOVER }
  let mode = if v6 { c.IPV6_PMTUDISC_PROBE } else { c.IP_PMTUDISC_PROBE }
  linux.setsockopt_int(fd, level, discover, mode)
  linux.setsockopt_int(fd, level, if v6 { c.IPV6_RECVERR } else { c.IP_RECVERR }, 1)
  linux.setsockopt_int(fd, level, if v6 { c.IPV6_RECVHOPLIMIT } else { c.IP_RECVTTL }, 1)
  linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_TIMESTAMPNS, 1)
  Ok(fd)
}

## Run tracepath for the operands in `argv`.
export proc execute(argv: List[Str]) {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {status: 255},
      ipv4: {form: "-4", default: false},
      ipv6: {form: "-6", default: false},
      both: {form: "-b", default: false},
      numeric: {form: "-n", default: false},
      help: {form: "-h", default: false, stop: true},
      length: {form: "-l LENGTH"},
      hops: {form: "-m HOPS"},
      port: {form: "-p PORT"},
      version: {form: "-V", default: false, stop: true},
      destinations: {form: "...DESTINATION"},
    },
  )?
  if opts.help {
    io.write_stderr("\n" + USAGE)
    exit 255
  }
  if opts.version {
    gnu.version("tracepath")
    return
  }
  if opts.ipv4 and opts.ipv6 {
    gnu.error("Only one -4 or -6 option may be specified")
    exit 1
  }
  if opts.destinations.len() != 1 {
    io.write_stderr("\n" + USAGE)
    exit 255
  }
  let family = if opts.ipv6 { "ipv6" } else if opts.ipv4 { "ipv4" } else { "any" }
  let max_hops = if let text = opts.hops { long_option(text, 0, 255) } else { 30 }
  var base_port = if let text = opts.port { long_option(text, 0, 65535) } else { 44444 }
  var name = opts.destinations[0]
  let slash = name.find("/")
  if let at = slash {
    base_port = long_option(name.byte_slice(at + 1), 0, 65535)
    name = name.byte_slice(0, at)
  }
  guard let found = dns.resolve_host(name, family) else {
    gnu.error(f"{name}: Name or service not known")
    exit 1
  }
  guard ! found.is_empty() else {
    gnu.error(f"{name}: Name or service not known")
    exit 1
  }
  let v6 = found[0].family == "ipv6"
  let overhead = if v6 { 48 } else { 28 }
  var mtu = if v6 { 128000 } else { 65535 }
  if let text = opts.length {
    mtu = long_option(text, 0, INT_MAX)
    if mtu <= overhead {
      gnu.error(f"pktlen must be within: {overhead} < value <= {INT_MAX}")
      exit 1
    }
  }
  let setup: Setup = {v6: v6, address: found[0].addr, base_port: base_port, overhead: overhead, numeric: opts.numeric, both: opts.both}
  let fd = match open_probe_socket(v6) {
    Ok(opened) => opened
    Err(failure) => {
      gnu.error(f"socket: {gnu.strerror(failure)}")
      exit 1
    }
  }
  defer unix.close_fd(fd)
  let c = linux.net_constants()
  let clock = icmp.open_clock()?
  var walk: Walk = {mtu: mtu, hops_to: -1, hops_from: -1, history: {}, slot: 0, names: {}}
  var finished = false
  var ttl = 1
  while ttl <= max_hops and ! finished {
    linux.setsockopt_int(fd, if v6 { c.SOL_IPV6 } else { c.SOL_IP }, if v6 { c.IPV6_UNICAST_HOPS } else { c.IP_TTL }, ttl)
    var progress = -1
    for _ in range(3) {
      let before = walk.mtu
      let reading = probe(fd, ttl, walk, setup, clock)?
      walk = reading.walk
      progress = reading.progress
      if walk.mtu == before { break }
    }
    if progress == 0 {
      finished = true
    } else {
      if progress < 0 { gnu.write_text(f"{ttl:>2}:  no reply\n") }
      ttl += 1
    }
  }
  if ! finished {
    gnu.write_text(f"     Too many hops: pmtu {walk.mtu}\n")
  }
  var resume = f"     Resume: pmtu {walk.mtu} "
  if walk.hops_to >= 0 { resume += f"hops {walk.hops_to} " }
  if walk.hops_from >= 0 { resume += f"back {walk.hops_from} " }
  gnu.write_text(resume + "\n")
}
