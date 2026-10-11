##! ping: ICMP and ICMPv6 echo over the typed socket boundary, with the
##! iputils option surface, report format, and exit statuses (0 replies
##! received, 1 none or fewer than a counted run with a deadline, 2 error).
use gnu
use icmp

const USAGE = """Usage
  ping [options] <destination>...

Options:
  <destination>      DNS name or IP address
  -4                 use IPv4
  -6                 use IPv6
  -a                 use audible ping
  -A                 use adaptive ping
  -b                 allow pinging broadcast
  -B                 sticky source address
  -c <count>         stop after <count> replies
  -d                 use SO_DEBUG socket option
  -D                 print timestamps
  -e <identifier>    define identifier for ping session; uses a raw socket
  -h                 print help and exit
  -H                 force reverse DNS name resolution, override -n
  -I <interface>     either interface name or address
  -i <interval>      seconds between sending each packet
  -l <preload>       send <preload> number of packages while waiting replies
  -L                 suppress loopback of multicast packets
  -m <mark>          tag the packets going out
  -M <pmtud opt>     define path MTU discovery, can be one of <do|dont|want|probe>
  -n                 no reverse DNS name resolution, override -H
  -O                 report outstanding replies
  -p <pattern>       contents of padding byte
  -q                 quiet output
  -Q <tclass>        use quality of service <tclass> bits
  -r                 bypass the normal routing tables
  -s <size>          use <size> as number of data bytes to be sent
  -S <size>          use <size> as SO_SNDBUF socket option value
  -t <ttl>           define time to live
  -v                 verbose output
  -V                 print version and exit
  -w <deadline>      reply wait <deadline> in seconds
  -W <timeout>       time to wait for response
"""

type Options = {
  ipv4: Bool, ipv6: Bool, audible: Bool, adaptive: Bool, broadcast: Bool, sticky: Bool,
  count: Str?, debug: Bool, timestamps: Bool, identifier: Str?, help: Bool, resolve: Bool,
  interface: Str?, interval: Str?, preload: Str?, no_loopback: Bool, mark: Str?, hint: Str?,
  numeric: Bool, outstanding: Bool, pattern: Str?, quiet: Bool, tos: Str?, dontroute: Bool,
  size: Str?, sndbuf: Str?, ttl: Str?, verbose: Bool, version: Bool, deadline: Str?, linger: Str?,
  destinations: List[Str]
}

# Everything the command line decides, validated and in plain units.
type Plan = {
  family: Str, count: Int, interval_ms: Int, linger_ms: Int, deadline_s: Int,
  size: Int?, preload: Int, ttl: Int?, tos: Int?, mark: Int?, sndbuf: Int?, hint: Str?,
  pattern: Bytes, identifier: Int?, interface: Str?, source: Str?
}

# One destination after name resolution. `named` is true when the user gave a
# host name rather than an address, which is when replies are shown by name.
type Target = {given: Str, address: Str, family: Str, named: Bool}

const LONG_MAX = 9223372036854775807
const INT_MAX = 2147483647
const MIN_USER_INTERVAL_MS = 2
const DEFAULT_LINGER_MS = 10000
const DECIMAL = rx"^[ \t]*[+-]?[0-9]+$"
const SECONDS = rx"^[ \t]*([0-9]*\.?[0-9]*)(.*)$"
const IPV4_LITERAL = rx"^[0-9]{1,3}(\.[0-9]{1,3}){3}$"

# A strtol-style integer option: nothing but an optionally signed decimal
# number, inside LOW..=HIGH. A violation ends the program with status 1 as
# iputils does.
proc long_option(text: Str, low: Int, high: Int) [process, env] -> Int {
  guard DECIMAL.matches(text) else {
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

# Seconds given as a decimal number, in milliseconds. `rest` is whatever
# follows the number, which iputils tolerates with a warning.
type SecondsValue = {ms: Int, rest: Str}

proc seconds_option(text: Str) [process, env] -> SecondsValue {
  let parts = SECONDS.captures(text)
  let number = if parts.is_empty() { "" } else { parts[1] }
  let rest = if parts.is_empty() { text } else { parts[2] }
  var value = 0.0
  if number != "" and number != "." {
    value = number.parse_float() ?? 0.0
  }
  let ms = (value * 1000.0).round() ?? INT_MAX
  {ms: ms, rest: rest}
}

pure is_address(text: Str) -> Bool {
  ":" in text or IPV4_LITERAL.matches(text)
}

# Reads one `-Q` value the way strtol does with base 0: 0x prefix hexadecimal,
# otherwise decimal.
proc tos_option(text: Str) [process, env] -> Int {
  let lower = text.trim().lower()
  var value: Int? = null
  if lower.starts_with("0x") {
    var number = 0
    var valid = lower.byte_len() > 2
    for index in range(2, lower.byte_len()) {
      let digit = "0123456789abcdef".find(lower.byte_slice(index, length: 1))
      if digit == null { valid = false } else { number = number * 16 + digit }
    }
    if valid { value = number }
  } else if DECIMAL.matches(text) {
    let unsigned = if lower.starts_with("+") { lower.byte_slice(1) } else { lower }
    if let Ok(number) = unsigned.parse_int() { value = number }
  }
  guard let number = value else {
    gnu.error(f"invalid argument: '{text}'")
    exit 1
  }
  guard number >= 0 and number <= 255 else {
    gnu.error(f"the decimal value of TOS bits must be in range 0-255: {number}")
    exit 2
  }
  number
}

proc hint_value(text: Str) [process, env] -> Str {
  guard text.lower() in ["do", "dont", "want", "probe"] else {
    gnu.error(f"invalid -M argument: {text}")
    exit 2
  }
  text.lower()
}

proc plan_of(opts: Options, forced: Str) [process, env, io, error] -> Plan {
  if opts.ipv4 and opts.ipv6 {
    gnu.error("only one -4 or -6 option may be specified")
    exit 2
  }
  let family = if opts.ipv6 { "ipv6" } else if opts.ipv4 { "ipv4" } else { forced }
  if forced == "ipv6" and opts.ipv4 {
    gnu.error("only one -4 or -6 option may be specified")
    exit 2
  }
  let count = if let text = opts.count { long_option(text, 1, LONG_MAX) } else { 0 }
  var interval_ms = 1000
  if let text = opts.interval {
    let seconds = seconds_option(text)
    if seconds.rest != "" {
      eprint f"{gnu.prog()}: option argument contains garbage: {seconds.rest}"
      eprint f"{gnu.prog()}: this will become fatal error in the future"
    }
    if seconds.ms < 0 {
      gnu.error(f"bad timing interval: {text}")
      exit 2
    }
    interval_ms = seconds.ms
  }
  if interval_ms < MIN_USER_INTERVAL_MS and unix.id()?.uid != 0 {
    gnu.error(f"cannot flood, minimal interval for user must be >= {MIN_USER_INTERVAL_MS} ms, use -i 0.002 (or higher)")
    exit 2
  }
  var linger_ms = DEFAULT_LINGER_MS
  if let text = opts.linger {
    let seconds = seconds_option(text)
    if text.trim().starts_with("-") or seconds.rest != "" or seconds.ms > INT_MAX {
      gnu.error(f"bad linger time: {text}")
      exit 2
    }
    linger_ms = seconds.ms
  }
  let deadline = if let text = opts.deadline { long_option(text, 0, INT_MAX) } else { 0 }
  let size: Int? = if let text = opts.size { long_option(text, 0, INT_MAX) } else { null }
  let preload = if let text = opts.preload { long_option(text, 1, INT_MAX) } else { 1 }
  if preload > 3 and unix.id()?.uid != 0 {
    gnu.error(f"cannot set preload to value greater than 3: {preload}")
    exit 2
  }
  let ttl: Int? = if let text = opts.ttl { long_option(text, 0, 255) } else { null }
  let tos: Int? = if let text = opts.tos { tos_option(text) } else { null }
  let mark: Int? = if let text = opts.mark { long_option(text, 0, 4294967295) } else { null }
  let sndbuf: Int? = if let text = opts.sndbuf { long_option(text, 1, INT_MAX) } else { null }
  let identifier: Int? = if let text = opts.identifier { long_option(text, 0, 65535) } else { null }
  let hint: Str? = if let text = opts.hint { hint_value(text) } else { null }
  var pattern = b""
  if let text = opts.pattern {
    guard let parsed = icmp.pattern_bytes(text) else {
      gnu.error(f"patterns must be specified as hex digits: {text}")
      exit 2
    }
    pattern = parsed
    if ! opts.quiet {
      gnu.write_text("PATTERN: 0x" + [hex_byte(value) for value in pattern].join("") + "\n")
    }
  }
  var interface: Str? = null
  var source: Str? = null
  if let text = opts.interface {
    if is_address(text) { source = text } else { interface = text }
  }
  {family: family, count: count, interval_ms: interval_ms, linger_ms: linger_ms, deadline_s: deadline, size: size, preload: preload, ttl: ttl, tos: tos, mark: mark, sndbuf: sndbuf, hint: hint, pattern: pattern, identifier: identifier, interface: interface, source: source}
}

pure min_int(left: Int, right: Int) -> Int {
  if left < right { left } else { right }
}

pure max_int(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

pure hex_byte(value: Int) -> Str {
  "0123456789abcdef".byte_slice(value / 16, length: 1) + "0123456789abcdef".byte_slice(value % 16, length: 1)
}

# Microseconds as a reply's "time=" value: three decimals under 1 ms, two
# under 10 ms, one under 100 ms, whole milliseconds above.
pure reply_time_text(us: Int) -> Str {
  if us >= 100000 - 50 {
    return f"{(us + 500) / 1000}"
  }
  if us >= 10000 - 5 {
    let tenths = (us + 50) / 100
    return f"{tenths / 10}.{tenths % 10}"
  }
  if us >= 1000 {
    let hundredths = (us + 5) / 10
    return f"{hundredths / 100}.{hundredths % 100:02}"
  }
  f"{us / 1000}.{us % 1000:03}"
}

# Microseconds as milliseconds with exactly three decimals.
pure millis_text(us: Int) -> Str {
  f"{us / 1000}.{us % 1000:03}"
}

pure isqrt(value: Int) -> Int {
  if value <= 0 { return 0 }
  var root = value.float().sqrt().floor() ?? 0
  while root * root > value { root -= 1 }
  while (root + 1) * (root + 1) <= value { root += 1 }
  root
}

# Running totals of one ping run, in microseconds, in the units iputils keeps.
type Stats = {
  sent: Int, received: Int, duplicates: Int, errors: Int, corrupted: Int,
  rtt_min: Int, rtt_max: Int, rtt_sum: Int, rtt_sum2: Int, pipe: Int, ewma8: Int,
  first_ns: Int, last_ns: Int
}

pure percent_loss(stats: Stats) -> Int {
  if stats.sent == 0 { return 0 }
  (stats.sent - stats.received) * 100 / stats.sent
}

# The "--- HOST ping statistics ---" block.
pure summary_text(given: Str, stats: Stats, adaptive: Bool, interval_ms: Int) -> Str {
  var text = f"\n--- {given} ping statistics ---\n{stats.sent} packets transmitted, {stats.received} received"
  if stats.duplicates > 0 { text += f", +{stats.duplicates} duplicates" }
  if stats.corrupted > 0 { text += f", +{stats.corrupted} corrupted" }
  if stats.errors > 0 { text += f", +{stats.errors} errors" }
  # Whole milliseconds, rounded, between the first probe and the last event.
  let elapsed_us = (stats.last_ns - stats.first_ns) / 1000
  if stats.sent > 0 {
    text += f", {percent_loss(stats)}% packet loss, time {(elapsed_us + 500) / 1000}ms"
  }
  text += "\n"
  var comma = ""
  var line = ""
  let samples = stats.received + stats.duplicates
  if stats.received > 0 and samples > 0 {
    let mean = stats.rtt_sum / samples
    let variance = stats.rtt_sum2 / samples - mean * mean
    line += f"rtt min/avg/max/mdev = {millis_text(stats.rtt_min)}/{millis_text(mean)}/{millis_text(stats.rtt_max)}/{millis_text(isqrt(variance))} ms"
    comma = ", "
  }
  if stats.pipe > 1 {
    line += f"{comma}pipe {stats.pipe}"
    comma = ", "
  }
  if stats.received > 0 and (adaptive or interval_ms == 0) and stats.sent > 1 {
    let gap = elapsed_us / (stats.sent - 1)
    line += f"{comma}ipg/ewma {gap / 1000}.{gap % 1000:03}/{stats.ewma8 / 8000}.{stats.ewma8 / 8 % 1000:03} ms"
  }
  text + line + "\n"
}

# The status a finished (or interrupted) run reports.
pure exit_status(stats: Stats, plan: Plan) -> Int {
  if stats.received == 0 { return 1 }
  if plan.count > 0 and plan.deadline_s > 0 and stats.received < plan.count { return 1 }
  0
}

proc resolve(name: Str, family: Str) [net, process, env] -> Target {
  let literal = is_address(name)
  let mismatch = (literal and family == "ipv4" and ":" in name) or (literal and family == "ipv6" and ":" not in name)
  if mismatch {
    gnu.error(f"{name}: Address family for hostname not supported")
    exit 2
  }
  let wanted = if family == "ipv4" { "ipv4" } else if family == "ipv6" { "ipv6" } else { "any" }
  guard let found = dns.resolve_host(name, wanted) else {
    gnu.error(f"{name}: Name or service not known")
    exit 2
  }
  guard ! found.is_empty() else {
    gnu.error(f"{name}: Name or service not known")
    exit 2
  }
  let first = found[0]
  {given: name, address: first.addr, family: if first.family == "ipv6" { "inet6" } else { "inet" }, named: ! literal}
}

# The address of a reply with the name its reverse lookup found, if any.
pure label(address: Str, names: Map[Str], show: Bool) -> Str {
  if ! show { return address }
  let known = names.get(address) ?? ""
  if known == "" { return address }
  f"{known} ({address})"
}

proc reverse_name(address: Str) [net] -> Str {
  let found = dns.reverse(address)
  if let Ok(list) = found {
    if ! list.is_empty() { return list[0] }
  }
  ""
}

pure errno_text(errno: Int) -> Str {
  match errno {
    1 => "Operation not permitted"
    13 => "Permission denied"
    22 => "Invalid argument"
    71 => "Protocol error"
    90 => "Message too long"
    100 => "Network is down"
    101 => "Network is unreachable"
    111 => "Connection refused"
    113 => "No route to host"
    else => f"Unknown error {errno}"
  }
}

# One entry of the socket's error queue, in the wording iputils uses.
proc report_error(failure: icmp.QueuedError, target: Target, names: Map[Str], show: Bool, v6: Bool) [net, process, env, io] {
  if failure.origin == 1 {
    if failure.errno == 90 {
      gnu.error(f"local error: message too long, mtu={failure.info}")
    } else {
      gnu.error(f"local error: {errno_text(failure.errno)}")
    }
    return
  }
  let sequence = (icmp.parse_echo(failure.payload) ?? {kind: 0, code: 0, identifier: 0, sequence: 0, payload: b""}).sequence
  let who = if failure.offender == "" { target.address } else { failure.offender }
  let text = if v6 { icmp.icmp6_text(failure.kind, failure.code, failure.info) } else { icmp.icmp4_text(failure.kind, failure.code, failure.info) }
  gnu.write_text(f"From {label(who, names, show)} icmp_seq={sequence} {text}\n")
}

# What the routing table says about a destination, asked of a throwaway UDP
# socket: the source address a probe would carry, and whether the destination
# is a broadcast address (a connect without SO_BROADCAST is refused).
type Route = {source: Str, broadcast: Bool}

proc probe_route(target: Target, interface: Str?, requested: Str?) [process, net, error] -> Result[Route, Error] {
  let c = linux.net_constants()
  let domain = if target.family == "inet6" { c.AF_INET6 } else { c.AF_INET }
  let fd = linux.socket(domain, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)
  if let device = interface {
    linux.setsockopt_bytes(fd, c.SOL_SOCKET, c.SO_BINDTODEVICE, bytes.from_text(device))
  }
  if let Err(failure) = linux.connect(fd, {family: target.family, address: target.address, port: 1025}) {
    if gnu.errno(failure) == 13 { return Ok({source: requested ?? "", broadcast: true}) }
    return Err(failure)
  }
  Ok({source: requested ?? linux.getsockname(fd)?.address, broadcast: false})
}

proc configure(sock: icmp.EchoSocket, plan: Plan, opts: Options, target: Target) [process, net, env, error] {
  let c = linux.net_constants()
  let v6 = target.family == "inet6"
  let ip_level = if v6 { c.SOL_IPV6 } else { c.SOL_IP }
  if let device = plan.interface {
    if let Err(failure) = linux.setsockopt_bytes(sock.fd, c.SOL_SOCKET, c.SO_BINDTODEVICE, bytes.from_text(device)) {
      gnu.error(f"SO_BINDTODEVICE {device}: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if let hops = plan.ttl {
    let unicast = if v6 { c.IPV6_UNICAST_HOPS } else { c.IP_TTL }
    if let Err(failure) = linux.setsockopt_int(sock.fd, ip_level, unicast, hops) {
      gnu.error(f"cannot set unicast time-to-live: {gnu.strerror(failure)}")
      exit 2
    }
    let multicast = if v6 { c.IPV6_MULTICAST_HOPS } else { c.IP_MULTICAST_TTL }
    if let Err(failure) = linux.setsockopt_int(sock.fd, ip_level, multicast, hops) {
      gnu.error(f"cannot set multicast time-to-live: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if let bits = plan.tos {
    let option = if v6 { c.IPV6_TCLASS } else { c.IP_TOS }
    if let Err(failure) = linux.setsockopt_int(sock.fd, ip_level, option, bits) {
      gnu.error(f"cannot set tos/tclass {bits}: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if let hint = plan.hint {
    # The "want" mode has the same number in both families.
    let values = if v6 {
      {do: c.IPV6_PMTUDISC_DO, dont: c.IPV6_PMTUDISC_DONT, want: c.IP_PMTUDISC_WANT, probe: c.IPV6_PMTUDISC_PROBE}
    } else {
      {do: c.IP_PMTUDISC_DO, dont: c.IP_PMTUDISC_DONT, want: c.IP_PMTUDISC_WANT, probe: c.IP_PMTUDISC_PROBE}
    }
    let mode = match hint {
      "do" => values.do
      "dont" => values.dont
      "want" => values.want
      else => values.probe
    }
    let option = if v6 { c.IPV6_MTU_DISCOVER } else { c.IP_MTU_DISCOVER }
    if let Err(failure) = linux.setsockopt_int(sock.fd, ip_level, option, mode) {
      gnu.error(f"cannot set path MTU discovery: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if let value = plan.mark {
    if let Err(failure) = linux.setsockopt_int(sock.fd, c.SOL_SOCKET, c.SO_MARK, value) {
      gnu.error(f"SO_MARK: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if let value = plan.sndbuf {
    if let Err(failure) = linux.setsockopt_int(sock.fd, c.SOL_SOCKET, c.SO_SNDBUF, value) {
      gnu.error(f"SO_SNDBUF: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if opts.debug {
    if let Err(failure) = linux.setsockopt_int(sock.fd, c.SOL_SOCKET, c.SO_DEBUG, 1) {
      gnu.error(f"SO_DEBUG: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if opts.dontroute {
    if let Err(failure) = linux.setsockopt_int(sock.fd, c.SOL_SOCKET, c.SO_DONTROUTE, 1) {
      gnu.error(f"SO_DONTROUTE: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if opts.broadcast {
    if let Err(failure) = linux.setsockopt_int(sock.fd, c.SOL_SOCKET, c.SO_BROADCAST, 1) {
      gnu.error(f"SO_BROADCAST: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if opts.no_loopback {
    let option = if v6 { c.IPV6_MULTICAST_LOOP } else { c.IP_MULTICAST_LOOP }
    if let Err(failure) = linux.setsockopt_int(sock.fd, ip_level, option, 0) {
      gnu.error(f"cannot disable multicast loopback: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if let address = plan.source {
    let sticky_address = address
    if let Err(failure) = linux.bind(sock.fd, {family: target.family, address: sticky_address, port: 0}) {
      gnu.error(f"bind: {gnu.strerror(failure)}")
      exit 2
    }
  }
}

pure payload_limit(family: Str) -> Int {
  if family == "inet6" { 65527 } else { 65507 }
}

# One run of ping against one destination. Returns the exit status.
proc ping_target(target: Target, plan: Plan, opts: Options) [net, process, env, time, io, error] -> Int {
  let size = plan.size ?? 56
  if size > payload_limit(target.family) {
    gnu.error(f"invalid -s value: '{size}': out of range: 0 <= value <= {payload_limit(target.family)}")
    exit 1
  }
  let v6 = target.family == "inet6"
  let sock = match icmp.open_echo(target.family, plan.identifier != null) {
    Ok(opened) => opened
    Err(failure) => {
      eprint f"{gnu.prog()}: socktype: SOCK_RAW"
      gnu.error(f"socket: {gnu.strerror(failure)}")
      if gnu.errno(failure) == 1 or gnu.errno(failure) == 13 {
        eprint f"{gnu.prog()}: => missing cap_net_raw+p capability or setuid?"
      }
      exit 2
    }
  }
  defer unix.close_fd(sock.fd)
  let c = linux.net_constants()
  icmp.enable_ancillary(sock)
  configure(sock, plan, opts, target)
  let route = match probe_route(target, plan.interface, plan.source) {
    Ok(found) => found
    Err(failure) => {
      gnu.error(f"connect: {gnu.strerror(failure)}")
      exit 2
    }
  }
  if route.broadcast and ! opts.broadcast {
    gnu.error("Do you want to ping broadcast? Then -b. If not, check your local firewall rules")
    exit 2
  }
  let wants_source = plan.interface != null or plan.source != null or opts.sticky
  if opts.sticky and plan.source == null and route.source != "" {
    if let Err(failure) = linux.bind(sock.fd, {family: target.family, address: route.source, port: 0}) {
      gnu.error(f"bind icmp socket: {gnu.strerror(failure)}")
      exit 2
    }
  }
  let clock = icmp.open_clock()?
  let identifier = plan.identifier ?? process.current_pid()? % 65536
  let fill = icmp.echo_fill(size, plan.pattern)?
  let checksummed = sock.raw and ! v6

  let from_text = if wants_source { f"from {route.source} {plan.interface ?? ""}: " } else { "" }
  let shape = if v6 { f"{size} data bytes" } else { f"{size}({size + 28}) bytes of data." }
  let shown_address = target.address
  gnu.write_text(f"PING {target.given} ({shown_address}) {from_text}{shape}\n")
  if route.broadcast {
    gnu.write_text("WARNING: pinging broadcast address\n")
  }
  if opts.verbose {
    eprint f"{gnu.prog()}: sock{if v6 { "6" } else { "4" }}.fd: {sock.fd} (socktype: {if sock.raw { "SOCK_RAW" } else { "SOCK_DGRAM" }}), hints.ai_family: {if v6 { "AF_INET6" } else { "AF_INET" }}"
    eprint f"{gnu.prog()}: ai->ai_family: {if v6 { "AF_INET6" } else { "AF_INET" }}, ai->ai_canonname: '{target.given}'"
  }

  let show_names = (target.named or opts.resolve) and ! opts.numeric
  var names: Map[Str] = {}
  let destination = {family: target.family, address: target.address, port: 0}
  let started_ms = time.now()
  var stats: Stats = {sent: 0, received: 0, duplicates: 0, errors: 0, corrupted: 0, rtt_min: 0, rtt_max: 0, rtt_sum: 0, rtt_sum2: 0, pipe: 0, ewma8: 0, first_ns: 0, last_ns: 0}
  var seen: Set[Int] = set.empty()
  var sent_at: Map[Int, Int] = {}
  var next_send_ms = started_ms
  var sending = true
  var drain_deadline_ms = -1
  var last_sent_ms = started_ms
  var queued_copies = 0
  var delivered = 0
  var finished = false
  defer {
    gnu.write_text(summary_text(target.given, stats, opts.adaptive, plan.interval_ms))
  }
  # The entry script's interrupt hook cannot see this proc's locals, so the
  # status an interrupted run should report is kept in the evaluator
  # environment, which it can read: 1 until the first reply, then 0.
  e"XSH_PING_STATUS" = "1"
  while ! finished {
    let now = time.now()
    if plan.deadline_s > 0 and now - started_ms >= plan.deadline_s * 1000 {
      finished = true
      continue
    }
    if sending and now >= next_send_ms {
      let burst = if stats.sent == 0 { plan.preload } else { 1 }
      for _ in range(burst) {
        let sequence = (stats.sent + 1) % 65536
        if opts.outstanding and stats.sent > 0 and stats.sent % 65536 not in seen {
          gnu.write_text(f"no answer yet for icmp_seq={stats.sent % 65536}\n")
        }
        let stamp_ms = time.now()
        let stamp = bytes.concat([bytes.pack_le(stamp_ms / 1000, 8)?, bytes.pack_le(stamp_ms % 1000 * 1000, 8)?])
        let message = icmp.echo_request(target.family, identifier, sequence, icmp.echo_stamped(fill, stamp), checksummed)?
        let marked = icmp.clock_mark(clock)?
        let sent = linux.sendto(sock.fd, message, destination)
        let sent_ns = icmp.clock_collect(clock, marked)?
        let first_ns = if stats.sent == 0 { sent_ns } else { stats.first_ns }
        stats = {...stats, sent: stats.sent + 1, first_ns: first_ns, last_ns: sent_ns}
        if let Err(failure) = sent {
          stats = {...stats, errors: stats.errors + 1}
          gnu.error(f"sendmsg: {gnu.strerror(failure)}")
          # The kernel also queues a failed send as a local error; the send's
          # own failure above is the report, so the queued copy is dropped.
          queued_copies += 1
        } else {
          sent_at = sent_at.set(sequence, sent_ns)
          seen = seen.remove(sequence)
          delivered += 1
        }
      }
      last_sent_ms = now
      next_send_ms = now + plan.interval_ms
      if plan.count > 0 and stats.sent >= plan.count and plan.deadline_s == 0 {
        sending = false
      }
    }
    if plan.count > 0 and stats.received >= plan.count {
      finished = true
      continue
    }
    if ! sending {
      # Nothing is left to wait for when every probe that left was answered
      # or none could be sent.
      if delivered <= stats.received {
        finished = true
        continue
      }
      if drain_deadline_ms < 0 {
        let wait_ms = if stats.received > 0 { min_int(plan.linger_ms, max_int(2 * stats.rtt_max / 1000, 1)) } else { plan.linger_ms }
        drain_deadline_ms = now + wait_ms
      }
      if now >= drain_deadline_ms {
        finished = true
        continue
      }
    }
    var pause_ms = 1000
    if sending { pause_ms = max_int(next_send_ms - now, 0) }
    if drain_deadline_ms >= 0 { pause_ms = min_int(pause_ms, max_int(drain_deadline_ms - now, 0)) }
    if plan.deadline_s > 0 { pause_ms = min_int(pause_ms, max_int(started_ms + plan.deadline_s * 1000 - now, 0)) }
    let events = unix.poll_fd(sock.fd, ["readable"], pause_ms)?
    if "error" in events {
      loop {
        let queued = icmp.read_error(sock.fd)?
        guard let failure = queued else { break }
        if failure.origin == 1 and queued_copies > 0 {
          queued_copies -= 1
          continue
        }
        stats = {...stats, errors: stats.errors + 1, last_ns: icmp.clock_ns(clock)?}
        if opts.quiet { continue }
        report_error(failure, target, names, show_names, v6)
      }
    }
    if "readable" in events {
      loop {
        let arrived = icmp.receive(sock.fd, c.MSG_DONTWAIT)
        guard let got = arrived else { break }
        let offset = if sock.raw and ! v6 { icmp.ip_header_length(got.data) } else { 0 }
        let message = got.data.slice(offset)
        guard let echo = icmp.parse_echo(message) else { continue }
        if echo.kind != icmp.reply_type(target.family) { continue }
        if sock.raw and echo.identifier != identifier { continue }
        guard sent_at.get(echo.sequence) is Ok(_) else { continue }
        if checksummed and ! icmp.checksum_ok(message) {
          stats = {...stats, corrupted: stats.corrupted + 1}
          continue
        }
        let arrival_ns = got.stamp_ns ?? icmp.clock_ns(clock)?
        let departed_ns = sent_at.get(echo.sequence)?
        let rtt = max_int((arrival_ns - departed_ns) / 1000, 0)
        let duplicate = echo.sequence in seen
        seen = seen.add(echo.sequence)
        stats = {...stats, last_ns: arrival_ns}
        if duplicate {
          stats = {...stats, duplicates: stats.duplicates + 1}
        } else {
          stats = {...stats, received: stats.received + 1}
          e"XSH_PING_STATUS" = "0"
          if opts.adaptive {
            # The next probe leaves as soon as the round trip has been seen.
            next_send_ms = last_sent_ms + max_int(rtt / 1000, MIN_USER_INTERVAL_MS)
          }
        }
        let total = stats.received + stats.duplicates
        let tmin = if total == 1 or rtt < stats.rtt_min { rtt } else { stats.rtt_min }
        let tmax = if rtt > stats.rtt_max { rtt } else { stats.rtt_max }
        let ewma = if stats.ewma8 == 0 { rtt * 8 } else { stats.ewma8 + rtt - stats.ewma8 / 8 }
        let outstanding = stats.sent - echo.sequence + 1
        stats = {...stats, rtt_min: tmin, rtt_max: tmax, rtt_sum: stats.rtt_sum + rtt, rtt_sum2: stats.rtt_sum2 + rtt * rtt, ewma8: ewma, pipe: max_int(stats.pipe, outstanding)}
        if opts.quiet { continue }
        if got.address not in names and show_names {
          names = names.set(got.address, reverse_name(got.address))
        }
        let ttl_text = if let hops = got.ttl { f" ttl={hops}" } else if sock.raw and ! v6 { f" ttl={got.data.byte_at(8) ?? 0}" } else { "" }
        let stamp_text = if opts.timestamps { f"[{arrival_ns / 1000000000}.{arrival_ns / 1000 % 1000000:06}] " } else { "" }
        let bell = if opts.audible { "\u{7}" } else { "" }
        let dup_text = if duplicate { " (DUP!)" } else { "" }
        gnu.write_text(f"{bell}{stamp_text}{message.len()} bytes from {label(got.address, names, show_names)}: icmp_seq={echo.sequence}{ttl_text} time={reply_time_text(rtt)} ms{dup_text}\n")
      }
    }
  }
  exit_status(stats, plan)
}

## Run ping (`forced` is "any", or "ipv6" for ping6) over every destination in
## `argv` and end the script with the worst status.
export proc execute(argv: List[Str], forced: Str) {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {
        status: 2,
        unsupported: {
          "-f": "flood ping is not available",
          "-3": "round-trip precision selection is not available",
          "-C": "connecting the socket is not available",
          "-U": "user-to-user latency is not available",
          "-R": "IPv4 record route is not available",
          "-T": "IPv4 timestamp options are not available",
          "-N": "ICMPv6 node information queries are not available",
          "-F": "IPv6 flow labels are not available",
        },
      },
      ipv4: {form: "-4", default: false},
      ipv6: {form: "-6", default: false},
      audible: {form: "-a", default: false},
      adaptive: {form: "-A", default: false},
      broadcast: {form: "-b", default: false},
      sticky: {form: "-B", default: false},
      count: {form: "-c COUNT"},
      debug: {form: "-d", default: false},
      timestamps: {form: "-D", default: false},
      identifier: {form: "-e IDENTIFIER"},
      help: {form: "-h", default: false, stop: true},
      resolve: {form: "-H", default: false},
      interface: {form: "-I INTERFACE"},
      interval: {form: "-i INTERVAL"},
      preload: {form: "-l PRELOAD"},
      no_loopback: {form: "-L", default: false},
      mark: {form: "-m MARK"},
      hint: {form: "-M HINT"},
      numeric: {form: "-n", default: false},
      outstanding: {form: "-O", default: false},
      pattern: {form: "-p PATTERN"},
      quiet: {form: "-q", default: false},
      tos: {form: "-Q TCLASS"},
      dontroute: {form: "-r", default: false},
      size: {form: "-s SIZE"},
      sndbuf: {form: "-S SNDBUF"},
      ttl: {form: "-t TTL"},
      verbose: {form: "-v", default: false},
      version: {form: "-V", default: false, stop: true},
      deadline: {form: "-w DEADLINE"},
      linger: {form: "-W TIMEOUT"},
      destinations: {form: "...DESTINATION"},
    },
  )?
  if opts.help {
    io.write_stderr("\n" + USAGE)
    exit 2
  }
  if opts.version {
    gnu.version("ping")
    return
  }
  if opts.destinations.is_empty() {
    gnu.error("usage error: Destination address required")
    exit 2
  }
  let plan = plan_of(opts, forced)
  var status = 0
  for name in opts.destinations {
    let target = resolve(name, plan.family)
    let code = ping_target(target, plan, opts)
    if code > status { status = code }
  }
  exit status
}
