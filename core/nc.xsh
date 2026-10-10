#!/bin/xsh
##! nc: a netcat-openbsd compatible TCP, UDP, and Unix-domain stream client,
##! listener, and port scanner.
##!
##! The relay moves bytes between standard input, the socket, and standard
##! output with the typed socket primitives of the `linux` module. `unix.poll_fd`
##! waits on one descriptor, so the relay checks standard input without
##! blocking and then waits a short slice on the socket; input is therefore
##! noticed within one slice (`WAIT_SLICE_MS`) instead of at once. Everything
##! else follows the OpenBSD utility as packaged by Debian: option letters,
##! diagnostics, and exit statuses were pinned against its transcripts.
##!
##! Not provided, and refused with a diagnostic instead of being ignored: the
##! proxy options (-X, -x, -P), TCP MD5 signatures (-S), file descriptor
##! passing (-F), DCCP (-Z), and alternate routing tables (-V; Linux has none).
use lib.gnu

const BUFSIZE = 16384
const WAIT_SLICE_MS = 10
const ACCEPT_SLICE_MS = 100
const SERVICES_FILE = p"/etc/services"
const OPTIONS_WITH_VALUE = "IiMmOPpqsTVWwXx"

const USAGE = """usage: nc [-46CDdFhklNnrStUuvZz] [-I length] [-i interval] [-M ttl]
	  [-m minttl] [-O length] [-P proxy_username] [-p source_port]
	  [-q seconds] [-s sourceaddr] [-T keyword] [-V rtable] [-W recvlimit]
	  [-w timeout] [-X proxy_protocol] [-x proxy_address[:port]]
	  [destination] [port]
"""

const HELP = """OpenBSD netcat (Debian patchlevel 1.234-1)
usage: nc [-46CDdFhklNnrStUuvZz] [-I length] [-i interval] [-M ttl]
	  [-m minttl] [-O length] [-P proxy_username] [-p source_port]
	  [-q seconds] [-s sourceaddr] [-T keyword] [-V rtable] [-W recvlimit]
	  [-w timeout] [-X proxy_protocol] [-x proxy_address[:port]]
	  [destination] [port]
	Command Summary:
		-4		Use IPv4
		-6		Use IPv6
		-b		Allow broadcast
		-C		Send CRLF as line-ending
		-D		Enable the debug socket option
		-d		Detach from stdin
		-F		Pass socket fd
		-h		This help text
		-I length	TCP receive buffer length
		-i interval	Delay interval for lines sent, ports scanned
		-k		Keep inbound sockets open for multiple connects
		-l		Listen mode, for inbound connects
		-M ttl		Outgoing TTL / Hop Limit
		-m minttl	Minimum incoming TTL / Hop Limit
		-N		Shutdown the network socket after EOF on stdin
		-n		Suppress name/port resolutions
		-O length	TCP send buffer length
		-P proxyuser	Username for proxy authentication
		-p port		Specify local port for remote connects
		-q secs		quit after EOF on stdin and delay of secs
		-r		Randomize remote ports
		-S		Enable the TCP MD5 signature option
		-s sourceaddr	Local source address
		-T keyword	TOS value
		-t		Answer TELNET negotiation
		-U		Use UNIX domain socket
		-u		UDP mode
		-V rtable	Specify alternate routing table
		-v		Verbose
		-W recvlimit	Terminate after receiving a number of packets
		-w timeout	Timeout for connects and final net reads
		-X proto	Proxy protocol: "4", "4A", "5" (SOCKS) or "connect"
		-x addr[:port]	Specify proxy address and port
		-Z		DCCP mode
		-z		Zero-I/O mode [used for scanning]
	Port numbers can be individual or ranges: lo-hi [inclusive]
"""

# The utility dies of a termination signal; the evaluator's cancellation would
# end the script with status 3 instead.
on INT [] {
  exit 130
}

on TERM [] {
  exit 143
}

type Options = {
  family: Str,
  broadcast: Bool,
  crlf: Bool,
  debug: Bool,
  nostdin: Bool,
  passfd: Bool,
  rcvbuf: Int?,
  interval: Int,
  keep: Bool,
  listen: Bool,
  ttl: Int?,
  minttl: Int?,
  shutdown: Bool,
  numeric: Bool,
  sndbuf: Int?,
  proxy_user: Str?,
  srcport: Str?,
  quit: Int,
  random: Bool,
  md5: Bool,
  srcaddr: Str?,
  telnet: Bool,
  tos: Int?,
  unix: Bool,
  udp: Bool,
  rtable: Str?,
  verbose: Bool,
  recvlimit: Int,
  timeout: Int,
  proxy_proto: Str?,
  proxy_addr: Str?,
  dccp: Bool,
  scan: Bool,
}

## One decimal number checked against a range; `problem` is empty on success.
type Number = {value: Int, problem: Str}
type Service = {name: Str, port: Int, aliases: List[Str]}
type Address = {addr: Str, family: Str, name: Str}
type Lookup = {rows: List[Address], problem: Str}
type Parsed = {opts: Options, operands: List[Str]}

# A failure here is deliberately dropped: the caller has no recourse (a
# descriptor that will not close, a stream already shut) and a statement-level
# failure would end the relay.
proc attempt(outcome: Result[Unit]) -> Unit {
  if let Err(_) = outcome { }
}

# A diagnostic: `nc: MESSAGE` on standard error.
proc say(message: Str) [process, io] -> Unit {
  gnu.error(message)
  attempt(io.flush_stderr())
}

# A verbose progress line, which the utility prints without its name.
proc note(message: Str) [process, io] -> Unit {
  eprint $message
  attempt(io.flush_stderr())
}

proc fatal(message: Str) [process, io] -> Unit {
  say(message)
  exit 1
}

proc usage() [process, io] -> Unit {
  eprint USAGE.byte_slice(0, USAGE.byte_len() - 1)
  attempt(io.flush_stderr())
  exit 1
}

# strtonum(3): a decimal integer in [low, high], else why it is refused.
pure number_in(raw: Str, low: Int, high: Int) -> Number {
  let text = raw.trim()
  if text != raw.trim() or ! rx"^[+-]?[0-9]+$".matches(text) {
    return {value: 0, problem: "invalid"}
  }
  let parsed = text.parse_int()
  if let Err(_) = parsed {
    return {value: 0, problem: if text.starts_with("-") { "too small" } else { "too large" }}
  }
  let value = parsed ?? 0
  if value < low { return {value: 0, problem: "too small"} }
  if value > high { return {value: 0, problem: "too large"} }
  {value: value, problem: ""}
}

# An option's numeric argument, or the diagnostic strtonum-based parsing gives.
proc option_number(label: Str, raw: Str, low: Int, high: Int) [process, io] -> Int {
  let parsed = number_in(raw, low, high)
  if parsed.problem != "" {
    fatal(f"{label} {parsed.problem}: {raw}")
  }
  parsed.value
}

pure tos_keyword(word: Str) -> Int? {
  match word {
    "af11" => 40
    "af12" => 48
    "af13" => 56
    "af21" => 72
    "af22" => 80
    "af23" => 88
    "af31" => 104
    "af32" => 112
    "af33" => 120
    "af41" => 136
    "af42" => 144
    "af43" => 152
    "critical" => 160
    "cs0" => 0
    "cs1" => 32
    "cs2" => 64
    "cs3" => 96
    "cs4" => 128
    "cs5" => 160
    "cs6" => 192
    "cs7" => 224
    "ef" => 184
    "inetcontrol" => 192
    "lowdelay" => 16
    "netcontrol" => 224
    "reliability" => 4
    "throughput" => 8
    _ => null
  }
}

# -T takes a keyword or a number, which strtoul(3) reads in any base.
proc tos_value(word: Str) [process, io] -> Int {
  if let named = tos_keyword(word) {
    return named
  }
  var number = -1
  if rx"^0[xX][0-9a-fA-F]+$".matches(word) {
    number = word.parse_int() ?? -1
  } else if rx"^[0-9]+$".matches(word) {
    number = word.parse_int() ?? -1
  }
  if number < 0 or number > 255 {
    fatal(f"illegal tos value {word}")
  }
  number
}

proc parse_options(argv: List[Str]) [process, io] -> Parsed {
  var opts: Options = {
    family: "any", broadcast: false, crlf: false, debug: false, nostdin: false,
    passfd: false, rcvbuf: null, interval: 0, keep: false, listen: false,
    ttl: null, minttl: null, shutdown: false, numeric: false, sndbuf: null,
    proxy_user: null, srcport: null, quit: -1, random: false, md5: false,
    srcaddr: null, telnet: false, tos: null, unix: false, udp: false,
    rtable: null, verbose: false, recvlimit: 0, timeout: -1, proxy_proto: null,
    proxy_addr: null, dccp: false, scan: false,
  }
  var index = 0
  while index < argv.len() {
    let word = argv[index]
    if word == "--" {
      index += 1
      break
    }
    if ! word.starts_with("-") or word == "-" { break }
    var at = 1
    while at < word.byte_len() {
      let letter = word.byte_slice(at, 1)
      at += 1
      var value = ""
      if OPTIONS_WITH_VALUE.find(letter) != null {
        if at < word.byte_len() {
          value = word.byte_slice(at)
        } else {
          index += 1
          if index >= argv.len() {
            say(f"option requires an argument: {letter}")
            usage()
          }
          value = argv[index]
        }
        at = word.byte_len()
      }
      match letter {
        "4" => opts.family = "inet"
        "6" => opts.family = "inet6"
        "b" => opts.broadcast = true
        "C" => opts.crlf = true
        "D" => opts.debug = true
        "d" => opts.nostdin = true
        "F" => opts.passfd = true
        "h" => {
          eprint HELP.byte_slice(0, HELP.byte_len() - 1)
          attempt(io.flush_stderr())
          exit 0
        }
        "k" => opts.keep = true
        "l" => opts.listen = true
        "N" => opts.shutdown = true
        "n" => opts.numeric = true
        "r" => opts.random = true
        "S" => opts.md5 = true
        "t" => opts.telnet = true
        "U" => opts.family = "unix"
        "u" => opts.udp = true
        "v" => opts.verbose = true
        "Z" => opts.dccp = true
        "z" => opts.scan = true
        "I" => opts.rcvbuf = option_number("TCP receive window", value, 0, 2147483647)
        "O" => opts.sndbuf = option_number("TCP send window", value, 0, 2147483647)
        "i" => opts.interval = option_number("interval", value, 0, 2147483647)
        "q" => opts.quit = option_number("quit timer", value, -2147483648, 2147483647)
        "W" => opts.recvlimit = option_number("receive limit", value, 1, 2147483647)
        "w" => opts.timeout = option_number("timeout", value, 0, 2147483) * 1000
        "M" => {
          let parsed = number_in(value, 0, 255)
          if parsed.problem == "too large" { fatal("ttl is too large") }
          if parsed.problem != "" { fatal("ttl is invalid") }
          opts.ttl = parsed.value
        }
        "m" => {
          let parsed = number_in(value, 0, 255)
          if parsed.problem == "too large" { fatal("minttl is too large") }
          if parsed.problem != "" { fatal("minttl is invalid") }
          opts.minttl = parsed.value
        }
        "P" => opts.proxy_user = value
        "p" => opts.srcport = value
        "s" => opts.srcaddr = value
        "T" => opts.tos = tos_value(value)
        "V" => opts.rtable = value
        "X" => {
          if value not in ["4", "4A", "5", "connect"] {
            fatal("unsupported proxy protocol")
          }
          opts.proxy_proto = value
        }
        "x" => opts.proxy_addr = value
        _ => {
          say(f"unrecognized option: {letter}")
          usage()
        }
      }
    }
    index += 1
  }
  {opts: opts, operands: argv[index..]}
}

proc services_for(proto: Str) [fs] -> List[Service] {
  let text = SERVICES_FILE.read_text() ?? ""
  var found: List[Service] = []
  for raw in text.lines() {
    let note = raw.find("#") ?? -1
    let line = if note >= 0 { raw.byte_slice(0, note) } else { raw }
    let fields = line.fields()
    if fields.len() < 2 { continue }
    let pair = fields[1].split("/")
    if pair.len() != 2 or pair[1] != proto { continue }
    let number = pair[0].parse_int() ?? -1
    if number < 0 { continue }
    found += [{name: fields[0], port: number, aliases: fields[2..]}]
  }
  found
}

proc service_port(name: Str, proto: Str) [fs] -> Int? {
  for entry in services_for(proto) {
    if entry.name == name or name in entry.aliases { return entry.port }
  }
  null
}

proc service_name(port: Int, proto: Str) [fs] -> Str? {
  for entry in services_for(proto) {
    if entry.port == port { return entry.name }
  }
  null
}

pure proto_name(opts: Options) -> Str {
  if opts.udp { "udp" } else { "tcp" }
}

# A port operand: a number, a lo-hi range, or a service name. A range given
# backwards is scanned in ascending order.
proc build_ports(spec: Str, opts: Options) [fs, process, io] -> List[Int] {
  let text = spec.trim()
  let dash = text.find("-") ?? -1
  if dash > 0 and rx"^[0-9]".matches(text) {
    let low = number_in(text.byte_slice(0, dash), 1, 65535)
    let high_text = text.byte_slice(dash + 1)
    if low.problem != "" {
      fatal(f"port number {low.problem}: {text.byte_slice(0, dash)}")
    }
    let high = number_in(high_text, 1, 65535)
    if high.problem != "" {
      fatal(f"port number {high.problem}: {high_text}")
    }
    let first = if low.value < high.value { low.value } else { high.value }
    let last = if low.value < high.value { high.value } else { low.value }
    return [number for number in range(first, last + 1)]
  }
  if rx"^[0-9]".matches(text) or rx"^-[0-9]*$".matches(text) {
    let single = number_in(text, 1, 65535)
    if single.problem != "" {
      fatal(f"port number {single.problem}: {text}")
    }
    return [single.value]
  }
  if let named = service_port(text, proto_name(opts)) {
    return [named]
  }
  fatal(f"service \"{text}\" unknown")
  []
}

# The numeric port of a -p value, which getaddrinfo(3) also resolves by name.
proc source_port(spec: Str, opts: Options) [fs, process, io] -> Int {
  let parsed = number_in(spec, 0, 65535)
  if parsed.problem == "" { return parsed.value }
  if ! rx"^[0-9]".matches(spec) {
    if let named = service_port(spec, proto_name(opts)) {
      return named
    }
  }
  fatal("getaddrinfo: Unrecognized service")
  0
}

proc shuffled(ports: List[Int]) [fs, process, io] -> List[Int] {
  var out = ports
  var at = out.len() - 1
  while at > 0 {
    let noise = bytes.read_at(p"/dev/urandom", 0, 4) ?? b"\x00\x00\x00\x00"
    let pick = (bytes.unpack_le(noise, 4) ?? 0) % (at + 1)
    let held = out[at]
    out[at] = out[pick]
    out[pick] = held
    at -= 1
  }
  out
}

pure numeric_host(host: Str) -> Bool {
  if host.find(":") != null { return true }
  rx"^(0[xX][0-9a-fA-F]+|[0-9]+)(\.(0[xX][0-9a-fA-F]+|[0-9]+)){0,3}$".matches(host)
}

# The text after the last ": " of a resolver failure, which is the
# getaddrinfo(3) wording the utility prints.
pure resolver_reason(message: Str) -> Str {
  let parts = message.split(": ")
  parts[-1]
}

proc lookup(host: Str, opts: Options) [net] -> Lookup {
  if host == "" or (opts.numeric and ! numeric_host(host)) {
    return {rows: [], problem: "Name does not resolve"}
  }
  match dns.resolve_host(host) {
    Err(failure) => return {rows: [], problem: resolver_reason(failure.message)}
    Ok(rows) => {
      var usable: List[Address] = []
      for row in rows {
        if opts.family == "inet" and row.family != "ipv4" { continue }
        if opts.family == "inet6" and row.family != "ipv6" { continue }
        usable += [{addr: row.addr, family: row.family, name: row.name}]
      }
      if usable.is_empty() {
        return {rows: [], problem: "Name has no usable address"}
      }
      return {rows: usable, problem: ""}
    }
  }
}

# Names an address for the verbose messages: numeric under -n, else the
# reverse name when the resolver has one.
proc address_name(addr: Str, opts: Options) [net] -> Str {
  if opts.numeric { return addr }
  match dns.reverse(addr) {
    Ok(names) => return if names.is_empty() { addr } else { names[0] }
    Err(_) => return addr
  }
}

pure display_host(host: Str, addr: Str) -> Str {
  if host == addr { host } else { f"{host} ({addr})" }
}

pure socket_family(family: Str) -> Str {
  if family == "ipv6" or family == "inet6" { "inet6" } else { "inet" }
}

proc open_socket(family: Str, udp: Bool) [net, process, error] -> Result[Int] {
  let c = linux.net_constants()
  let domain = match family {
    "inet6" => c.AF_INET6
    "unix" => c.AF_UNIX
    _ => c.AF_INET
  }
  linux.socket(domain, if udp { c.SOCK_DGRAM } else { c.SOCK_STREAM })
}

# Socket options every connecting or listening socket takes before use.
proc tune_socket(fd: Int, family: Str, opts: Options) [net, process, io, error] -> Unit {
  let c = linux.net_constants()
  let v6 = family == "inet6"
  if opts.debug {
    if let Err(failure) = linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_DEBUG, 1) {
      fatal(gnu.strerror(failure))
    }
  }
  if opts.broadcast {
    if let Err(failure) = linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_BROADCAST, 1) {
      fatal(f"set SO_BROADCAST: {gnu.strerror(failure)}")
    }
  }
  if let size = opts.rcvbuf {
    if let Err(failure) = linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_RCVBUF, size) {
      fatal(f"set SO_RCVBUF: {gnu.strerror(failure)}")
    }
  }
  if let size = opts.sndbuf {
    if let Err(failure) = linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_SNDBUF, size) {
      fatal(f"set SO_SNDBUF: {gnu.strerror(failure)}")
    }
  }
  if let tos = opts.tos {
    let level = if v6 { c.SOL_IPV6 } else { c.SOL_IP }
    let option = if v6 { c.IPV6_TCLASS } else { c.IP_TOS }
    if let Err(failure) = linux.setsockopt_int(fd, level, option, tos) {
      fatal(f"set IP ToS: {gnu.strerror(failure)}")
    }
  }
  if let ttl = opts.ttl {
    let level = if v6 { c.SOL_IPV6 } else { c.SOL_IP }
    let option = if v6 { c.IPV6_UNICAST_HOPS } else { c.IP_TTL }
    if let Err(failure) = linux.setsockopt_int(fd, level, option, ttl) {
      fatal(f"set IP TTL: {gnu.strerror(failure)}")
    }
  }
  if let floor = opts.minttl {
    # IP_MINTTL and IPV6_MINHOPCOUNT are not in the named constants.
    let level = if v6 { c.SOL_IPV6 } else { c.SOL_IP }
    let option = if v6 { 73 } else { 21 }
    if let Err(failure) = linux.setsockopt_int(fd, level, option, floor) {
      fatal(f"set IP min TTL: {gnu.strerror(failure)}")
    }
  }
}

# Binds the -s address and -p port of an outgoing socket.
proc bind_source(fd: Int, family: Str, opts: Options) [net, fs, process, io, error] -> Unit {
  if opts.srcaddr == null and opts.srcport == null { return }
  var address = if family == "inet6" { "::" } else { "0.0.0.0" }
  if let given = opts.srcaddr {
    # The source address is resolved even under -n, which only guards the peer.
    let found = lookup(given, {...opts, numeric: false, family: if family == "inet6" { "inet6" } else { "inet" }})
    if found.rows.is_empty() {
      fatal(f"getaddrinfo: {found.problem}")
    }
    address = found.rows[0].addr
  }
  var port = 0
  if let given = opts.srcport {
    port = source_port(given, opts)
  }
  let c = linux.net_constants()
  if let Err(failure) = linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_REUSEADDR, 1) {
    fatal(f"set SO_REUSEADDR: {gnu.strerror(failure)}")
  }
  if let Err(failure) = linux.bind(fd, {family: family, address: address, port: port}) {
    fatal(f"bind failed: {gnu.strerror(failure)}")
  }
}

# One connect attempt, bounded by -w when given. The kernel reports a connect
# that ran out of its send timeout as EINPROGRESS; the utility names that
# ETIMEDOUT.
proc connect_bounded(fd: Int, target: Record, opts: Options) [net, process, error] -> Result[Unit] {
  let c = linux.net_constants()
  if opts.timeout < 0 {
    return linux.connect(fd, target)
  }
  linux.set_socket_timeout(fd, c.SO_SNDTIMEO, if opts.timeout == 0 { 1 } else { opts.timeout })?
  let outcome = linux.connect(fd, target)
  linux.set_socket_timeout(fd, c.SO_SNDTIMEO, 0)?
  if let Err(failure) = outcome {
    if failure.errno == 115 {
      return Err(error.failure("Operation timed out"))
    }
  }
  outcome
}

# Connects to HOST PORT, trying each address of the name in order, and returns
# the descriptor or -1 when every address failed.
proc connect_tcp_udp(host: Str, port: Int, opts: Options) [net, fs, process, io, error] -> Int {
  let found = lookup(host, opts)
  if found.rows.is_empty() {
    fatal(f"getaddrinfo for host \"{host}\" port {port}: {found.problem}")
  }
  for row in found.rows {
    let family = socket_family(row.family)
    let made = open_socket(family, opts.udp)
    if let Err(failure) = made {
      say(f"socket: {gnu.strerror(failure)}")
      continue
    }
    let fd = made ?? -1
    tune_socket(fd, family, opts)
    bind_source(fd, family, opts)
    let outcome = connect_bounded(fd, {family: family, address: row.addr, port: port}, opts)
    if let Err(failure) = outcome {
      if opts.verbose {
        say(f"connect to {display_host(host, row.addr)} port {port} ({proto_name(opts)}) failed: {gnu.strerror(failure)}")
      }
      attempt(unix.close_fd(fd))
      continue
    }
    return fd
  }
  -1
}

# Four one-byte datagrams, the last of which fails when an earlier one drew an
# ICMP port-unreachable: the only way the utility has to see a closed UDP port.
proc udp_probe(fd: Int) [net, process] -> Bool {
  var delivered = false
  for _ in range(4) {
    delivered = unix.write_fd(fd, bytes.from_text("X")) is Ok(_)
  }
  delivered
}

# Waits up to `slice_ms` for the descriptor; false when it was not readable.
proc readable(fd: Int, slice_ms: Int) [net, process] -> Bool {
  ! (unix.poll_fd(fd, ["readable"], slice_ms) ?? []).is_empty()
}

proc write_all(fd: Int, data: Bytes) [net, process, error] -> Result[Unit] {
  var rest = data
  while ! rest.is_empty() {
    let written = unix.write_fd(fd, rest)?
    rest = rest.slice(written)
  }
  Ok()
}

# -C: every bare LF becomes CR LF. `after_cr` carries a chunk's final byte.
pure add_carriage_returns(data: Bytes, after_cr: Bool) -> Bytes {
  if data.count_lines() == 0 { return data }
  var pieces: List[Bytes] = []
  var start = 0
  var previous_cr = after_cr
  for at in range(data.len()) {
    let byte = data.byte_at(at) ?? 0
    if byte == 10 and ! previous_cr {
      pieces += [data.slice(start, at - start), b"\r"]
      start = at
    }
    previous_cr = byte == 13
  }
  pieces += [data.slice(start)]
  bytes.concat(pieces)
}

# -t: refuses every telnet option a received chunk opens with (RFC 854).
proc answer_telnet(fd: Int, data: Bytes) [net, process, error] -> Unit {
  var at = 0
  while at + 2 < data.len() and data.byte_at(at) == 255 {
    let verb = data.byte_at(at + 1) ?? 0
    var reply = 0
    if verb == 251 or verb == 252 { reply = 254 }
    if verb == 253 or verb == 254 { reply = 252 }
    if reply != 0 {
      let option = data.byte_at(at + 2) ?? 0
      attempt(write_all(fd, bytes.from_ints([255, reply, option]) ?? b""))
      at += 3
    } else {
      at += 2
    }
  }
}

# Copies standard input to the socket and the socket to standard output until
# both directions are finished, like readwrite() of the utility. `listening`
# ends the relay as soon as the peer stops sending.
proc relay(fd: Int, opts: Options, listening: Bool) [net, process, time, io, error] -> Unit {
  let c = linux.net_constants()
  var stdin_open = ! opts.nostdin
  var net_in = true
  var net_out = true
  var stdout_open = true
  var pending: Bytes = b""
  var received = 0
  var timeout_ms = opts.timeout
  var after_cr = false
  var last_activity = time.now()
  if opts.nostdin and opts.quit >= 0 {
    timeout_ms = opts.quit * 1000
  }
  while true {
    if ! stdin_open and pending.is_empty() and ! net_in { break }
    if ! net_out and ! stdout_open { break }
    if listening and ! net_in and pending.is_empty() { break }
    var progressed = false

    if stdin_open and pending.is_empty() and net_out {
      if readable(0, 0) {
        match unix.read_fd(0, BUFSIZE) {
          Ok(data) => {
            if data.is_empty() {
              stdin_open = false
              if opts.quit >= 0 { timeout_ms = opts.quit * 1000 }
            } else {
              if opts.interval > 0 { time.sleep(time.seconds(opts.interval))? }
              if opts.crlf {
                pending = add_carriage_returns(data, after_cr)
                after_cr = data.byte_at(data.len() - 1) == 13
              } else {
                pending = data
              }
              progressed = true
            }
          }
          Err(_) => {
            stdin_open = false
            if opts.quit >= 0 { timeout_ms = opts.quit * 1000 }
          }
        }
      }
    }

    var wanted: List[Str] = []
    if net_in { wanted += ["readable"] }
    if ! pending.is_empty() and net_out { wanted += ["writable"] }
    var events: List[Str] = []
    if ! wanted.is_empty() {
      events = unix.poll_fd(fd, wanted, if progressed { 0 } else { WAIT_SLICE_MS }) ?? []
    } else if ! progressed {
      time.sleep(time.millis(WAIT_SLICE_MS))?
    }

    if "writable" in events and ! pending.is_empty() {
      match linux.sendto(fd, pending, null, c.MSG_DONTWAIT) {
        Ok(sent) => {
          pending = pending.slice(sent)
          progressed = true
        }
        Err(failure) => {
          if failure.errno != 11 {
            net_out = false
            pending = b""
          }
        }
      }
    } else if ("error" in events or "hangup" in events) and ! pending.is_empty() and "readable" not in events {
      net_out = false
      pending = b""
    }

    if net_in and ("readable" in events or "hangup" in events or "error" in events) {
      match unix.read_fd(fd, BUFSIZE) {
        Ok(data) => {
          if data.is_empty() {
            net_in = false
            attempt(linux.shutdown(fd, c.SHUT_RD))
          } else {
            progressed = true
            received += 1
            if opts.telnet { answer_telnet(fd, data) }
            if stdout_open {
              if let Err(_) = write_all(1, data) {
                stdout_open = false
                net_in = false
                attempt(linux.shutdown(fd, c.SHUT_RD))
              }
            }
            if opts.recvlimit > 0 and received >= opts.recvlimit {
              net_in = false
              attempt(linux.shutdown(fd, c.SHUT_RD))
            }
          }
        }
        Err(_) => {
          net_in = false
        }
      }
    }

    if ! net_in { stdout_open = false }
    if ! stdin_open and pending.is_empty() and net_out {
      if opts.shutdown {
        attempt(linux.shutdown(fd, c.SHUT_WR))
      }
      net_out = false
    }
    if ! net_out { stdin_open = false }

    if progressed {
      last_activity = time.now()
    } else if timeout_ms >= 0 and time.now() - last_activity >= timeout_ms {
      break
    }
  }
}

# Unix-domain: a socket path is its own address, so -p and the port operand
# have no meaning.
proc unix_address(socket_path: Str) -> Record {
  {family: "unix", address: socket_path}
}

proc connect_unix(socket_path: Str, opts: Options) [net, fs, process, io, error] -> Int {
  let made = open_socket("unix", opts.udp)
  if let Err(failure) = made {
    say(f"{socket_path}: {gnu.strerror(failure)}")
    return -1
  }
  let fd = made ?? -1
  tune_socket(fd, "unix", opts)
  if let local = opts.srcaddr {
    if let Err(failure) = linux.bind(fd, unix_address(local)) {
      fatal(f"bind failed: {gnu.strerror(failure)}")
    }
  } else if opts.udp {
    # A datagram client needs its own name for the listener to answer.
    let own = f"@nc-{process.current_pid() ?? 0}"
    if let Err(failure) = linux.bind(fd, unix_address(own)) {
      fatal(f"bind failed: {gnu.strerror(failure)}")
    }
  }
  if let Err(failure) = linux.connect(fd, unix_address(socket_path)) {
    say(f"{socket_path}: {gnu.strerror(failure)}")
    attempt(unix.close_fd(fd))
    return -1
  }
  fd
}

proc run_client(host: Str, port_specs: List[Str], opts: Options) [net, fs, process, time, io, error] -> Unit {
  var ports: List[Int] = []
  for spec in port_specs {
    ports = ports.extend(build_ports(spec, opts))
  }
  if opts.random { ports = shuffled(ports) }
  var status = 1
  var first = true
  for port in ports {
    if opts.interval > 0 and ! first { time.sleep(time.seconds(opts.interval))? }
    first = false
    let fd = connect_tcp_udp(host, port, opts)
    if fd < 0 { continue }
    status = 0
    if opts.verbose or opts.scan {
      if opts.udp {
        if opts.scan and ! udp_probe(fd) {
          status = 1
          attempt(unix.close_fd(fd))
          continue
        }
      }
      if ! opts.udp or opts.scan {
        let service: Str? = if opts.numeric { null } else { service_name(port, proto_name(opts)) }
        let peer = linux.getpeername(fd) ?? {address: host, family: "", port: port, raw: b"", scope_id: 0}
        say_connected(host, peer.address, port, proto_name(opts), service ?? "*")
      }
    }
    if ! opts.scan {
      relay(fd, opts, false)
    }
    attempt(unix.close_fd(fd))
  }
  exit status
}

proc say_connected(host: Str, addr: Str, port: Int, proto: Str, service: Str) [process, io] -> Unit {
  note(f"Connection to {display_host(host, addr)} {port} port [{proto}/{service}] succeeded!")
}

proc run_unix_client(socket_path: Str, opts: Options) [net, fs, process, time, io, error] -> Unit {
  let fd = connect_unix(socket_path, opts)
  if fd < 0 { exit 1 }
  if ! opts.scan {
    relay(fd, opts, false)
  }
  attempt(unix.close_fd(fd))
  exit 0
}

# Waits for the listening descriptor in short slices so a termination signal
# is seen between them.
proc wait_readable(fd: Int) [net, process] -> Unit {
  while ! readable(fd, ACCEPT_SLICE_MS) {
  }
}

proc listen_addresses(host: Str?, opts: Options) [net, process, io] -> List[Address] {
  if let name = host {
    let found = lookup(name, opts)
    if found.rows.is_empty() {
      fatal(f"getaddrinfo: {found.problem}")
    }
    return found.rows
  }
  var rows: List[Address] = []
  if opts.family != "inet6" { rows += [{addr: "0.0.0.0", family: "ipv4", name: ""}] }
  if opts.family != "inet" { rows += [{addr: "::", family: "ipv6", name: ""}] }
  rows
}

# Binds the first address that accepts it. Several listeners may share a port
# (SO_REUSEPORT), as the utility's do.
proc bind_listener(host: Str?, port: Int, opts: Options) [net, process, io, error] -> Int {
  let c = linux.net_constants()
  var last_failure = "Address not available"
  for row in listen_addresses(host, opts) {
    let family = socket_family(row.family)
    let made = open_socket(family, opts.udp)
    if let Err(failure) = made {
      last_failure = gnu.strerror(failure)
      continue
    }
    let fd = made ?? -1
    attempt(linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_REUSEADDR, 1))
    attempt(linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_REUSEPORT, 1))
    tune_socket(fd, family, opts)
    match linux.bind(fd, {family: family, address: row.addr, port: port}) {
      Ok(_) => return fd
      Err(failure) => {
        last_failure = gnu.strerror(failure)
        attempt(unix.close_fd(fd))
      }
    }
  }
  fatal(last_failure)
  -1
}

proc run_listener(host: Str?, port_spec: Str, opts: Options) [net, fs, process, time, io, error] -> Unit {
  let c = linux.net_constants()
  let ports = build_ports(port_spec, opts)
  if ports.len() != 1 { usage() }
  let port = ports[0]
  let fd = bind_listener(host, port, opts)
  let bound = linux.getsockname(fd) ?? {address: "", family: "inet", port: port, raw: b"", scope_id: 0}
  let bound_name = address_name(bound.address, opts)
  if opts.udp {
    if opts.verbose {
      note(f"Bound on {bound_name} {port}")
    }
    if ! opts.keep {
      wait_readable(fd)
      let first = linux.recvfrom(fd, 1, c.MSG_PEEK) ?? {address: bound, control: [], data: b"", flags: 0}
      if let Err(failure) = linux.connect(fd, first.address.raw) {
        fatal(gnu.strerror(failure))
      }
      if opts.verbose {
        note(f"Connection received on {address_name(first.address.address, opts)} {first.address.port}")
      }
    }
    relay(fd, opts, true)
    attempt(unix.close_fd(fd))
    exit 0
  }
  if let Err(failure) = linux.listen(fd, 1) {
    fatal(gnu.strerror(failure))
  }
  if opts.verbose {
    note(f"Listening on {bound_name} {port}")
  }
  while true {
    wait_readable(fd)
    match linux.accept(fd) {
      Err(failure) => fatal(gnu.strerror(failure))
      Ok(accepted) => {
        if opts.verbose {
          note(f"Connection received on {address_name(accepted.peer.address, opts)} {accepted.peer.port}")
        }
        if ! opts.keep {
          attempt(unix.close_fd(fd))
        }
        relay(accepted.fd, opts, true)
        attempt(unix.close_fd(accepted.fd))
        if ! opts.keep { exit 0 }
      }
    }
  }
}

proc run_unix_listener(socket_path: Str, opts: Options) [net, fs, process, time, io, error] -> Unit {
  let c = linux.net_constants()
  # The utility replaces whatever sits at the socket path.
  if fp"{socket_path}".exists() ?? false {
    attempt(fp"{socket_path}".remove())
  }
  let made = open_socket("unix", opts.udp)
  if let Err(failure) = made {
    fatal(gnu.strerror(failure))
  }
  let fd = made ?? -1
  tune_socket(fd, "unix", opts)
  if let Err(failure) = linux.bind(fd, unix_address(socket_path)) {
    fatal(gnu.strerror(failure))
  }
  if opts.verbose {
    note(f"Bound on {socket_path}")
  }
  if opts.udp {
    if ! opts.keep {
      wait_readable(fd)
      let first = linux.recvfrom(fd, 1, c.MSG_PEEK)
      if let Ok(datagram) = first {
        if datagram.address.raw.len() > 2 {
          attempt(linux.connect(fd, datagram.address.raw))
        }
      }
      if opts.verbose {
        note(f"Connection received on {socket_path}")
      }
    }
    relay(fd, opts, true)
    attempt(unix.close_fd(fd))
    exit 0
  }
  if let Err(failure) = linux.listen(fd, 1) {
    fatal(gnu.strerror(failure))
  }
  if opts.verbose {
    note(f"Listening on {socket_path}")
  }
  while true {
    wait_readable(fd)
    match linux.accept(fd) {
      Err(failure) => fatal(gnu.strerror(failure))
      Ok(accepted) => {
        if opts.verbose {
          note(f"Connection received on {socket_path}")
        }
        if ! opts.keep {
          attempt(unix.close_fd(fd))
        }
        relay(accepted.fd, opts, true)
        attempt(unix.close_fd(accepted.fd))
        if ! opts.keep { exit 0 }
      }
    }
  }
}

proc main(...argv: List[Str]) [net, fs, process, time, env, io, error] {
  let parsed = parse_options(argv)
  let opts = parsed.opts
  let operands = parsed.operands

  if opts.rtable != null {
    fatal("no alternate routing table support available")
  }
  if opts.passfd and opts.family == "unix" {
    fatal("cannot use -F and -U")
  }
  if opts.passfd {
    fatal("-F (file descriptor passing) is not supported")
  }
  if opts.dccp {
    fatal("-Z (DCCP) is not supported")
  }
  if opts.md5 {
    fatal("-S (TCP MD5 signatures) is not supported")
  }
  if opts.proxy_addr != null or opts.proxy_proto != null or opts.proxy_user != null {
    fatal("proxy connections (-X, -x, -P) are not supported")
  }

  var host: Str? = null
  var port_specs: List[Str] = []
  if opts.family == "unix" {
    if operands.len() == 0 { usage() }
    if operands.len() > 1 { fatal("cannot use port with -U") }
    if opts.srcport != null { usage() }
    host = operands[0]
  } else if operands.len() == 0 {
    if ! opts.listen { usage() }
    if let given = opts.srcport {
      port_specs = [given]
      host = opts.srcaddr
    } else {
      fatal("missing port number")
    }
  } else if operands.len() == 1 {
    if ! opts.listen { fatal("missing port number") }
    if opts.srcaddr != null or opts.srcport != null { usage() }
    port_specs = [operands[0]]
  } else {
    if opts.listen {
      if operands.len() != 2 or opts.srcaddr != null or opts.srcport != null { usage() }
    }
    host = operands[0]
    port_specs = operands[1..]
  }

  if opts.listen and opts.scan { fatal("cannot use -z and -l") }
  if ! opts.listen and opts.keep { fatal("must use -l with -k") }

  if opts.family == "unix" {
    let socket_path = host ?? ""
    if opts.listen {
      run_unix_listener(socket_path, opts)
    } else {
      run_unix_client(socket_path, opts)
    }
    return
  }
  if opts.listen {
    run_listener(host, port_specs[0], opts)
    return
  }
  run_client(host ?? "", port_specs, opts)
}
