#!/bin/xsh
##! ss: list sockets from the kernel's sock_diag interface.
##!
##! Output follows iproute2 `ss`: one header line and one row per socket, with
##! the Netid, State, Recv-Q, Send-Q, Local Address:Port and Peer Address:Port
##! columns padded to the widest cell. Unsupported options and tables fail
##! with a message instead of being ignored.
use lib.gnu
use lib.accounts
use lib.net_sockets as sockets
use lib.selinux

const USAGE = """Usage: ss [ OPTIONS ]
       ss [ OPTIONS ] [ FILTER ]
   -h, --help          this message
   -V, --version       output version information
   -n, --numeric       don't resolve service names
   -r, --resolve       resolve host names
   -a, --all           display all sockets
   -l, --listening     display listening sockets
   -o, --options       show timer information
   -e, --extended      show detailed socket information
   -m, --memory        show socket memory usage
   -p, --processes     show process using socket
   -i, --info          show internal TCP information
   -s, --summary       show socket usage summary
   -Z, --context       display task SELinux security contexts (fails: SELinux is not available)
   -4, --ipv4          display only IP version 4 sockets
   -6, --ipv6          display only IP version 6 sockets
   -t, --tcp           display only TCP sockets
   -u, --udp           display only UDP sockets
   -w, --raw           display only RAW sockets
   -x, --unix          display only Unix domain sockets
   -f, --family=FAMILY display sockets of type FAMILY
       FAMILY := {inet|inet6|unix}
   -H, --no-header     Suppress header line
   -Q, --no-queues     Suppress sending and receiving queue columns
   -O, --oneline       socket's data printed on a single line
   -A, --query=QUERY, --socket=QUERY
       QUERY := {all|inet|tcp|udp|raw|unix|unix_dgram|unix_stream|unix_seqpacket}[,QUERY]
   Packet, netlink, vsock, tipc, xdp, dccp, sctp and mptcp sockets are not listed;
   the options and tables that select them fail instead of printing nothing.
       FILTER := [ state STATE-FILTER ] [ EXPRESSION ]
       STATE-FILTER := {all|connected|synchronized|bucket|big|TCP-STATES}
         TCP-STATES := {established|syn-sent|syn-recv|fin-wait-{1,2}|time-wait|closed|close-wait|last-ack|listening|closing}
       EXPRESSION := [not] ( dst|src PREFIX | dport|sport [OP] PORT | dev NAME | fwmark MARK[/MASK] | cgroup PATH | autobound ) joined by and, or and ( )
"""

# Options that exist in the reference tool but have no implementation here.
# Each is refused by name so a script never believes it took effect.
const REFUSED: Map[Str, Str] = {
  "B": "bound-inactive sockets are not listed",
  "T": "thread listing is not implemented",
  "b": "BPF filter display is not implemented",
  "E": "continuous event display is not implemented",
  "N": "switching network namespaces is not implemented",
  "0": "packet sockets are not listed",
  "M": "MPTCP sockets are not listed",
  "S": "SCTP sockets are not listed",
  "d": "DCCP sockets are not listed",
  "K": "closing sockets is not implemented",
  "D": "raw TCP dumps are not implemented",
  "F": "reading a filter from a file is not implemented",
  "bound-inactive": "bound-inactive sockets are not listed",
  "threads": "thread listing is not implemented",
  "tipcinfo": "TIPC sockets are not listed",
  "tos": "TOS display is not implemented",
  "cgroup": "cgroup display is not implemented",
  "bpf": "BPF filter display is not implemented",
  "events": "continuous event display is not implemented",
  "net": "switching network namespaces is not implemented",
  "packet": "packet sockets are not listed",
  "mptcp": "MPTCP sockets are not listed",
  "sctp": "SCTP sockets are not listed",
  "dccp": "DCCP sockets are not listed",
  "tipc": "TIPC sockets are not listed",
  "vsock": "vsock sockets are not listed",
  "xdp": "XDP sockets are not listed",
  "kill": "closing sockets is not implemented",
  "diag": "raw TCP dumps are not implemented",
  "dump": "raw TCP dumps are not implemented",
  "filter": "reading a filter from a file is not implemented",
  "inet-sockopt": "inet socket option display is not implemented",
}

# Long options and the single-letter identity each one answers to.
const LONG: Map[Str, Str] = {
  "help": "h", "version": "V", "numeric": "n", "resolve": "r", "all": "a",
  "listening": "l", "options": "o", "extended": "e", "memory": "m",
  "processes": "p", "info": "i", "summary": "s", "context": "Z",
  "contexts": "z", "ipv4": "4", "ipv6": "6", "tcp": "t", "udp": "u",
  "raw": "w", "unix": "x", "family": "f", "no-header": "H",
  "no-queues": "Q", "oneline": "O", "query": "A", "socket": "A",
  "bound-inactive": "bound-inactive", "threads": "threads",
  "tipcinfo": "tipcinfo", "tos": "tos", "cgroup": "cgroup", "bpf": "bpf",
  "events": "events", "net": "net", "packet": "packet", "mptcp": "mptcp",
  "sctp": "sctp", "dccp": "dccp", "tipc": "tipc", "vsock": "vsock",
  "xdp": "xdp", "kill": "kill", "diag": "diag", "dump": "dump",
  "filter": "filter", "inet-sockopt": "inet-sockopt",
}

# Options that take a value, by identity.
const VALUED = ["f", "A", "N", "D", "F", "net", "diag", "dump", "filter"]
const SHORTS = "hVnralmoepisZz46tuwxfHQOAKBTbENDF0MSd"

const ALL_STATES = 4095
const LISTEN_BIT = 1024
const CLOSE_BIT = 128

type Config = {
  numeric: Bool,
  resolve: Bool,
  state_mode: Str,
  family: Int,
  tables: List[Str],
  timers: Bool,
  extended: Bool,
  memory: Bool,
  processes: Bool,
  info: Bool,
  summary: Bool,
  no_header: Bool,
  no_queues: Bool,
  oneline: Bool,
  context: Bool,
  help: Bool,
  version: Bool,
  words: List[Str],
}

# One condition or operator of a filter expression in postfix order.
type Cond = {
  kind: Str,
  op: Str,
  port: Int,
  family: Int,
  restrict: Int,
  prefix: Bytes,
  bits: Int,
  text: Str,
  mark: Int,
  mask: Int,
}

# Name databases by key: services by "port/proto", protocols by number,
# hosts by address text.
type Names = {services: Map[Str, Str], protocols: Map[Str, Str], hosts: Map[Str, Str]}

## Reports a usage problem with the option summary and ends with status 255,
## which is what the reference tool uses for every command-line error.
proc usage_fail(message: Str) [process, io, error] {
  eprint f"ss: {message}"
  eprint f"{USAGE}"
  exit 255
}

proc fail(message: Str, status: Int) [process, io, error] {
  eprint f"ss: {message}"
  exit status
}

proc reject(name: Str) [process, io, error] {
  let reason = REFUSED.get(name) ?? "this option is not supported"
  let shown = if name.byte_len() == 1 { f"-{name}" } else { f"--{name}" }
  fail(f"option '{shown}' is not supported: {reason}", 255)
}

# The tables a -A/-t/-u/-w/-x query names, added to those already chosen.
proc add_tables(chosen: List[Str], query: Str) [process, io, error] -> List[Str] {
  var tables = chosen
  for word in query.split(",") {
    var more: List[Str] = []
    match word {
      "all" => more = ["u_str", "u_dgr", "u_seq", "raw", "udp", "tcp"]
      "inet" => more = ["raw", "udp", "tcp"]
      "tcp" => more = ["tcp"]
      "udp" => more = ["udp"]
      "raw" => more = ["raw"]
      "unix" => more = ["u_str", "u_dgr", "u_seq"]
      "unix_stream" => more = ["u_str"]
      "unix_dgram" => more = ["u_dgr"]
      "unix_seqpacket" => more = ["u_seq"]
      "mptcp" | "sctp" | "dccp" | "packet" | "packet_raw" | "packet_dgram" | "netlink" | "vsock_stream" | "vsock_dgram" | "tipc" | "xdp" => fail(f"socket table '{word}' is not supported", 255)
      else => usage_fail(f"\"{word}\" is illegal socket table id")
    }
    for table in more {
      if table not in tables { tables += [table] }
    }
  }
  tables
}

type Opt = {identity: Str, value: Str}

type Scanned = {options: List[Opt], words: List[Str]}

# Splits the command line into options in order and the remaining words.
# Short options bundle, long options may be abbreviated to a unique prefix,
# and options and words may interleave until `--`.
proc scan_options(argv: List[Str]) [process, io, error] -> Scanned {
  var options: List[Opt] = []
  var words: List[Str] = []
  var index = 0
  var only_words = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if only_words or word == "-" or !word.starts_with("-") {
      words += [word]
    } else if word == "--" {
      only_words = true
    } else if word.starts_with("--") {
      let body = word.byte_slice(2)
      let split = body.find("=")
      let given = if let at = split { body.byte_slice(0, length: at) } else { body }
      var candidates: List[Str] = []
      for name in LONG.keys() {
        if name == given {
          candidates = [name]
          break
        }
        if name.starts_with(given) { candidates += [name] }
      }
      var identities: List[Str] = []
      for name in candidates {
        let identity: Str = LONG.get(name) ?? ""
        if identity not in identities { identities += [identity] }
      }
      if identities.is_empty() { usage_fail(f"unrecognized option '--{given}'") }
      if identities.len() > 1 { usage_fail(f"option '--{given}' is ambiguous") }
      let identity = identities[0]
      if identity in VALUED {
        var value = ""
        if let at = split {
          value = body.byte_slice(at + 1)
        } else if index < argv.len() {
          value = argv[index]
          index += 1
        } else {
          usage_fail(f"option '--{given}' requires an argument")
        }
        options += [{identity: identity, value: value}]
      } else {
        if split != null { usage_fail(f"option '--{given}' doesn't allow an argument") }
        options += [{identity: identity, value: ""}]
      }
    } else {
      let letters = word.byte_slice(1)
      var position = 0
      while position < letters.byte_len() {
        let letter = letters.byte_slice(position, length: 1)
        position += 1
        if SHORTS.find(letter) == null { usage_fail(f"invalid option -- '{letter}'") }
        if letter in VALUED {
          var value = letters.byte_slice(position)
          position = letters.byte_len()
          if value == "" {
            if index >= argv.len() { usage_fail(f"option requires an argument -- '{letter}'") }
            value = argv[index]
            index += 1
          }
          options += [{identity: letter, value: value}]
        } else {
          options += [{identity: letter, value: ""}]
        }
      }
    }
  }
  {options: options, words: words}
}

proc parse_args(argv: List[Str]) [process, io, error] -> Config {
  var numeric = false
  var resolve = false
  var state_mode = ""
  var family = 0
  var tables: List[Str] = []
  var timers = false
  var extended = false
  var memory = false
  var processes = false
  var info = false
  var summary = false
  var no_header = false
  var no_queues = false
  var oneline = false
  var context = false
  var help = false
  var version = false
  let scanned = scan_options(argv)
  for opt in scanned.options {
    if opt.identity in REFUSED.keys() { reject(opt.identity) }
    match opt.identity {
      "h" => help = true
      "V" | "v" => version = true
      "n" => numeric = true
      "r" => resolve = true
      "a" => state_mode = "all"
      "l" => state_mode = "listening"
      "o" => timers = true
      "e" => extended = true
      "m" => memory = true
      "p" => processes = true
      "i" => info = true
      "s" => summary = true
      "Z" | "z" => context = true
      "4" => family = sockets.AF_INET
      "6" => family = sockets.AF_INET6
      "t" => tables = add_tables(tables, "tcp")
      "u" => tables = add_tables(tables, "udp")
      "w" => tables = add_tables(tables, "raw")
      "x" => {
        tables = add_tables(tables, "unix")
        family = sockets.AF_UNIX
      }
      "H" => no_header = true
      "Q" => no_queues = true
      "O" => oneline = true
      "A" => tables = add_tables(tables, opt.value)
      "f" => {
        match opt.value {
          "inet" => family = sockets.AF_INET
          "inet6" => family = sockets.AF_INET6
          "unix" => {
            family = sockets.AF_UNIX
            tables = add_tables(tables, "unix")
          }
          "help" => {
            gnu.help(USAGE)
            exit 0
          }
          "link" | "netlink" | "vsock" | "tipc" | "xdp" => fail(f"address family '{opt.value}' is not supported", 255)
          else => fail(f"unknown family '{opt.value}'", 255)
        }
      }
      else => reject(opt.identity)
    }
  }
  {
    numeric: numeric, resolve: resolve, state_mode: state_mode, family: family,
    tables: tables, timers: timers, extended: extended, memory: memory,
    processes: processes, info: info, summary: summary, no_header: no_header,
    no_queues: no_queues, oneline: oneline, context: context, help: help,
    version: version, words: scanned.words,
  }
}

pure state_bit(state: Int) -> Int {
  var power = 1
  for _ in range(state) { power *= 2 }
  power
}

# The state mask a state keyword stands for, or -1 for an unknown keyword.
pure scan_state(word: Str) -> Int {
  let name = word.lower()
  match name {
    "close" | "closed" => state_bit(sockets.TCP_CLOSE)
    "syn-rcv" | "syn-recv" => state_bit(sockets.TCP_SYN_RECV)
    "establ" | "established" => state_bit(sockets.TCP_ESTABLISHED)
    "syn-sent" => state_bit(sockets.TCP_SYN_SENT)
    "fin-wait-1" => state_bit(sockets.TCP_FIN_WAIT1)
    "fin-wait-2" => state_bit(sockets.TCP_FIN_WAIT2)
    "time-wait" => state_bit(sockets.TCP_TIME_WAIT)
    "unconnected" => state_bit(sockets.TCP_CLOSE)
    "close-wait" => state_bit(sockets.TCP_CLOSE_WAIT)
    "last-ack" => state_bit(sockets.TCP_LAST_ACK)
    "listening" => state_bit(sockets.TCP_LISTEN)
    "closing" => state_bit(sockets.TCP_CLOSING)
    "connected" => ALL_STATES - state_bit(sockets.TCP_LISTEN) - state_bit(sockets.TCP_CLOSE) - 1
    "synchronized" => ALL_STATES - state_bit(sockets.TCP_LISTEN) - state_bit(sockets.TCP_CLOSE) - state_bit(sockets.TCP_SYN_SENT) - 1
    "bucket" => state_bit(sockets.TCP_SYN_RECV) + state_bit(sockets.TCP_TIME_WAIT)
    "big" => ALL_STATES - state_bit(sockets.TCP_SYN_RECV) - state_bit(sockets.TCP_TIME_WAIT) - 1
    "all" => ALL_STATES
    else => -1
  }
}

pure popcount(mask: Int) -> Int {
  var count = 0
  var rest = mask
  while rest > 0 {
    count += rest % 2
    rest = rest / 2
  }
  count
}

type Parse = {position: Int, output: List[Cond]}

pure blank_cond(kind: Str) -> Cond {
  {kind: kind, op: "", port: -1, family: 0, restrict: 0, prefix: b"", bits: -1, text: "", mark: 0, mask: 0}
}

pure port_op(word: Str) -> Str? {
  match word {
    "=" | "==" | "eq" => "="
    "!=" | "ne" | "neq" => "!="
    ">=" | "ge" | "geq" => ">="
    "<=" | "le" | "leq" => "<="
    ">" | "gt" => ">"
    "<" | "lt" => "<"
    else => null
  }
}

# The byte offset of the last occurrence of `needle`, or -1.
pure last_index(text: Str, needle: Str) -> Int {
  var found = -1
  var from = 0
  while from <= text.byte_len() {
    let at = text.find(needle, from)
    if at == null { break }
    found = at ?? -1
    from = at + 1
  }
  found
}

pure digit_value(letter: Str) -> Int {
  let at = "0123456789".find(letter)
  at ?? -1
}

# Parses dotted-quad text; null when it is not an IPv4 address.
pure parse_ipv4(text: Str) -> Bytes? {
  let parts = text.split(".")
  return null when parts.len() != 4

  var values: List[Int] = []
  for part in parts {
    if part == "" or part.byte_len() > 3 { return null }
    for letter in part {
      if digit_value(letter) < 0 { return null }
    }
    let number = part.parse_int() ?? -1
    if number < 0 or number > 255 { return null }
    values += [number]
  }
  if let Ok(data) = bytes.from_ints(values) { return data }
  null
}

# Parses colon-hexadecimal text, `::` compression and a dotted tail included;
# null when it is not an IPv6 address.
pure parse_ipv6(text: Str) -> Bytes? {
  return null when text.find(":") == null

  var body = text
  var tail: List[Int] = []
  if body.find(".") != null {
    let at = last_index(body, ":")
    return null when at < 0

    let v4 = parse_ipv4(body.byte_slice(at + 1))
    return null when v4 == null

    tail = [v4.byte_at(0) ?? 0, v4.byte_at(1) ?? 0, v4.byte_at(2) ?? 0, v4.byte_at(3) ?? 0]
    body = body.byte_slice(0, length: at + 1) + "0:0"
  }
  let halves = body.split("::")
  return null when halves.len() > 2

  var groups: List[Int] = []
  var after: List[Int] = []
  for index in range(halves.len()) {
    var target: List[Int] = []
    if halves[index] != "" {
      for part in halves[index].split(":") {
        return null when part == "" or part.byte_len() > 4

        let value = f"0x{part}".parse_int() ?? -1
        return null when value < 0 or value > 65535

        target += [value]
      }
    }
    if index == 0 { groups = target } else { after = target }
  }
  var all: List[Int] = []
  if halves.len() == 2 {
    let missing = 8 - groups.len() - after.len()
    return null when missing < 1

    all = groups
    for _ in range(missing) { all += [0] }
    all = all.extend(after)
  } else {
    all = groups
  }
  return null when all.len() != 8

  var raw: List[Int] = []
  for value in all {
    raw += [value / 256, value % 256]
  }
  if !tail.is_empty() {
    raw[12] = tail[0]
    raw[13] = tail[1]
    raw[14] = tail[2]
    raw[15] = tail[3]
  }
  if let Ok(data) = bytes.from_ints(raw) { return data }
  null
}

pure glob_match(pattern: Str, text: Str) -> Bool {
  # `*` matches any run and `?` one character, as fnmatch does without
  # bracket expressions.
  if pattern == "" { return text == "" }

  let first = pattern.byte_slice(0, length: 1)
  let rest = pattern.byte_slice(1)
  if first == "*" {
    for cut in range(text.byte_len() + 1) {
      if glob_match(rest, text.byte_slice(cut)) { return true }
    }
    return false
  }
  return false when text == ""

  if first == "?" or first == text.byte_slice(0, length: 1) {
    return glob_match(rest, text.byte_slice(1))
  }
  false
}

# Service-name lookups for a port keyword; see `Names`.
proc service_port(word: Str, tables: List[Str], names: Names) [fs, env, error] -> Int {
  let wanted_udp = "udp" in tables
  let wanted_tcp = "tcp" in tables
  var found = -1
  for entry in names.services.keys() {
    let parts = entry.split("/")
    if parts.len() != 2 { continue }
    if (names.services.get(entry) ?? "") == word {
      if (parts[1] == "udp" and wanted_udp) or (parts[1] == "tcp" and wanted_tcp) {
        found = parts[0].parse_int() ?? -1
        break
      }
    }
  }
  found
}

pure condition_start(word: Str) -> Bool {
  word in ["dst", "src", "dport", "sport", "dev", "fwmark", "cgroup", "autobound", "not", "!", "("]
}

proc syntax_error() [process, io, error] {
  eprint "ss: syntax error in filter expression"
  exit 255
}

proc parse_port(word: Str, tables: List[Str], names: Names) [fs, env, process, io, error] -> Int {
  let text = if word.starts_with(":") { word.byte_slice(1) } else { word }
  if text == "" or text == "*" { return -1 }

  var numeric = true
  for letter in text {
    if digit_value(letter) < 0 { numeric = false }
  }
  if numeric { return text.parse_int() ?? -1 }

  let found = service_port(text, tables, names)
  if found < 0 {
    eprint f"Error: \"{text}\" does not look like a port."
    eprint "Cannot parse dst/src address."
    exit 1
  }
  found
}

# Splits `HOST[:PORT]` the way the reference tool does: brackets quote an IPv6
# literal, `*` is a wildcard host, and otherwise the text after the last colon
# of the part before any `/` is the port.
proc parse_host(word: Str, family: Int, tables: List[Str], names: Names) [fs, env, process, io, error] -> Cond {
  var cond = blank_cond("host")
  var text = word
  if text.starts_with("unix:") or family == sockets.AF_UNIX {
    cond.family = sockets.AF_UNIX
    cond.text = if text.starts_with("unix:") { text.byte_slice(5) } else { text }
    return cond
  }
  for prefix in ["link:", "netlink:", "vsock:"] {
    if text.starts_with(prefix) {
      eprint f"ss: filter prefix '{prefix}' is not supported"
      exit 255
    }
  }
  var wanted = family
  if text.starts_with("inet:") {
    wanted = sockets.AF_INET
    cond.restrict = sockets.AF_INET
    text = text.byte_slice(5)
  } else if text.starts_with("inet6:") {
    wanted = sockets.AF_INET6
    cond.restrict = sockets.AF_INET6
    text = text.byte_slice(6)
  }
  var host = text
  var port_part = ""
  if text.starts_with("[") {
    let close = text.find("]") ?? -1
    if close < 0 {
      eprint f"Error: an inet prefix is expected rather than \"{word}\"."
      eprint "Cannot parse dst/src address."
      exit 1
    }
    host = text.byte_slice(1, length: close - 1)
    port_part = text.byte_slice(close + 1)
  } else if text.starts_with("*") {
    host = "*"
    port_part = text.byte_slice(1)
  } else {
    let slash = text.find("/")
    let head = if let at = slash { text.byte_slice(0, length: at) } else { text }
    let colon = last_index(head, ":")
    if colon >= 0 {
      host = text.byte_slice(0, length: colon)
      port_part = text.byte_slice(colon)
    }
  }
  if port_part.starts_with(":") { port_part = port_part.byte_slice(1) }
  if port_part != "" and port_part != "*" {
    cond.port = parse_port(port_part, tables, names)
  }
  if host != "" and host != "*" {
    var address = host
    var bits = -1
    let slash = host.find("/")
    if let at = slash {
      address = host.byte_slice(0, length: at)
      bits = host.byte_slice(at + 1).parse_int() ?? -1
    }
    let v4 = parse_ipv4(address)
    let v6 = parse_ipv6(address)
    if let data = v4 {
      cond.family = sockets.AF_INET
      cond.prefix = data
      cond.bits = if bits < 0 { 32 } else { bits }
    } else if let data = v6 {
      cond.family = sockets.AF_INET6
      cond.prefix = data
      cond.bits = if bits < 0 { 128 } else { bits }
    } else {
      eprint f"Error: an inet prefix is expected rather than \"{host}\"."
      eprint "Cannot parse dst/src address."
      exit 1
    }
    if (wanted == sockets.AF_INET and cond.family != sockets.AF_INET) or (wanted == sockets.AF_INET6 and cond.family != sockets.AF_INET6) {
      eprint f"Error: an inet prefix is expected rather than \"{host}\"."
      eprint "Cannot parse dst/src address."
      exit 1
    }
    if cond.bits == 0 { cond.bits = -1 }
  }
  cond
}

# A word that is not a keyword is lexed as a host (or, right after a port
# condition, a port) before the grammar rejects it, so a malformed one is
# reported as such and a well-formed one is a plain syntax error.
proc unknown_word(word: Str, after_port: Bool, family: Int, tables: List[Str], names: Names) [fs, env, process, io, error] {
  if after_port {
    let checked = parse_port(word, tables, names)
    if checked == -1 {
      eprint f"Error: \"{word}\" does not look like a port."
      eprint "Cannot parse dst/src address."
      exit 1
    }
  } else {
    let _ = parse_host(word, family, tables, names)
  }
  syntax_error()
}

proc parse_condition(words: List[Str], start: Int, family: Int, tables: List[Str], names: Names) [fs, env, process, io, error] -> Parse {
  var position = start
  if position >= words.len() { syntax_error() }
  let word = words[position]
  position += 1
  match word {
    "dst" | "src" => {
      if position >= words.len() { syntax_error() }
      var cond = parse_host(words[position], family, tables, names)
      cond.kind = word
      return {position: position + 1, output: [cond]}
    }
    "dport" | "sport" => {
      if position >= words.len() { syntax_error() }
      var op = "="
      if let given = port_op(words[position]) {
        op = given
        position += 1
        if position >= words.len() { syntax_error() }
      }
      var cond = blank_cond(word)
      cond.op = op
      cond.port = parse_port(words[position], tables, names)
      return {position: position + 1, output: [cond]}
    }
    "dev" => {
      if position >= words.len() { syntax_error() }
      var cond = blank_cond("dev")
      cond.text = words[position]
      return {position: position + 1, output: [cond]}
    }
    "fwmark" => {
      if position >= words.len() { syntax_error() }
      var cond = blank_cond("fwmark")
      let pieces = words[position].split("/")
      cond.mark = pieces[0].parse_int() ?? -1
      cond.mask = if pieces.len() > 1 { pieces[1].parse_int() ?? -1 } else { 4294967295 }
      if cond.mark < 0 or cond.mask < 0 {
        eprint f"Error: \"{words[position]}\" is invalid fwmark"
        eprint "Cannot parse dst/src address."
        exit 1
      }
      return {position: position + 1, output: [cond]}
    }
    "cgroup" => {
      if position >= words.len() { syntax_error() }
      var cond = blank_cond("cgroup")
      cond.text = words[position]
      return {position: position + 1, output: [cond]}
    }
    "autobound" => return {position: position, output: [blank_cond("autobound")]}
    else => {
      unknown_word(word, false, family, tables, names)
      return {position: position, output: []}
    }
  }
}

# expression := and-list { ("or" | "|" | "||") and-list }
# and-list   := unary { ["and" | "&" | "&&"] unary }
# unary      := ("not" | "!") unary | "(" expression ")" | condition
proc parse_unary(words: List[Str], start: Int, family: Int, tables: List[Str], names: Names) [fs, env, process, io, error] -> Parse {
  if start >= words.len() { syntax_error() }
  let word = words[start]
  if word == "not" or word == "!" {
    let inner = parse_unary(words, start + 1, family, tables, names)
    return {position: inner.position, output: inner.output.extend([blank_cond("not")])}
  }
  if word == "(" {
    let inner = parse_or(words, start + 1, family, tables, names)
    if inner.position >= words.len() or words[inner.position] != ")" { syntax_error() }
    return {position: inner.position + 1, output: inner.output}
  }
  parse_condition(words, start, family, tables, names)
}

proc parse_and(words: List[Str], start: Int, family: Int, tables: List[Str], names: Names) [fs, env, process, io, error] -> Parse {
  var current = parse_unary(words, start, family, tables, names)
  while current.position < words.len() {
    let next = words[current.position]
    var skip = 0
    if next in ["and", "&", "&&"] {
      skip = 1
    } else if !condition_start(next) {
      if next in [")", "or", "|", "||"] { break }
      var after_port = false
      for item in current.output {
        after_port = item.kind in ["sport", "dport"]
      }
      unknown_word(next, after_port, family, tables, names)
    }
    let right = parse_unary(words, current.position + skip, family, tables, names)
    current = {position: right.position, output: current.output.extend(right.output).extend([blank_cond("and")])}
  }
  current
}

proc parse_or(words: List[Str], start: Int, family: Int, tables: List[Str], names: Names) [fs, env, process, io, error] -> Parse {
  var current = parse_and(words, start, family, tables, names)
  while current.position < words.len() and words[current.position] in ["or", "|", "||"] {
    let right = parse_and(words, current.position + 1, family, tables, names)
    current = {position: right.position, output: current.output.extend(right.output).extend([blank_cond("or")])}
  }
  current
}

# A network interface by kernel index.
type Device = {index: Int, name: Str}

type Context = {low: Int, high: Int, devices: List[Device], cgroups: Map[Str, Int]}

pure prefix_match(prefix: Bytes, bits: Int, data: Bytes) -> Bool {
  var remaining = bits
  var index = 0
  while remaining > 0 {
    let have = data.byte_at(index) ?? 0
    let want = prefix.byte_at(index) ?? 0
    if remaining >= 8 {
      if have != want { return false }
      remaining -= 8
    } else {
      var keep = 1
      for _ in range(8 - remaining) { keep *= 2 }
      if have / keep != want / keep { return false }
      remaining = 0
    }
    index += 1
  }
  true
}

pure endpoint_match(cond: Cond, item: sockets.DiagSocket, port: Int, address: Bytes) -> Bool {
  if cond.family == sockets.AF_UNIX {
    return false when item.family != sockets.AF_UNIX

    return glob_match(cond.text, item.name ?? "*")
  }
  if cond.restrict != 0 and item.family != cond.restrict { return false }
  if cond.port >= 0 and cond.port != port { return false }
  if cond.bits < 0 { return true }
  return false when item.family == sockets.AF_UNIX

  if cond.family == sockets.AF_INET {
    if item.family == sockets.AF_INET { return prefix_match(cond.prefix, cond.bits, address) }
    # An IPv6 socket matches an IPv4 prefix through its v4-mapped form.
    var mapped = address.len() == 16
    for index in range(10) {
      if address.byte_at(index) != 0 { mapped = false }
    }
    if mapped and address.byte_at(10) == 255 and address.byte_at(11) == 255 {
      return prefix_match(cond.prefix, cond.bits, address.slice(12, 4))
    }
    return false
  }
  return false when item.family != sockets.AF_INET6

  prefix_match(cond.prefix, cond.bits, address)
}

pure autobound(item: sockets.DiagSocket, context: Context) -> Bool {
  if item.family == sockets.AF_UNIX {
    let name = item.name ?? ""
    return name.byte_len() == 6 and name.starts_with("@") and rx"^@[0-9a-f]{5}$".matches(name)
  }
  item.local_port != 0 and item.local_port >= context.low and item.local_port <= context.high
}

pure compare_port(op: Str, have: Int, want: Int) -> Bool {
  if want < 0 { return true }
  match op {
    "=" => have == want
    "!=" => have != want
    ">=" => have >= want
    "<=" => have <= want
    ">" => have > want
    else => have < want
  }
}

pure evaluate(program: List[Cond], item: sockets.DiagSocket, context: Context) -> Bool {
  var stack: List[Bool] = []
  for cond in program {
    match cond.kind {
      "and" => {
        let right = stack[-1]
        let left = stack[-2]
        stack = stack |> take(stack.len() - 2)
        stack += [left and right]
      }
      "or" => {
        let right = stack[-1]
        let left = stack[-2]
        stack = stack |> take(stack.len() - 2)
        stack += [left or right]
      }
      "not" => {
        let top = stack[-1]
        stack = stack |> take(stack.len() - 1)
        stack += [!top]
      }
      "src" => stack += [endpoint_match(cond, item, item.local_port, item.local)]
      "dst" => stack += [endpoint_match(cond, item, item.remote_port, item.remote)]
      "sport" => stack += [compare_port(cond.op, item.local_port, cond.port)]
      "dport" => stack += [compare_port(cond.op, item.remote_port, cond.port)]
      "autobound" => stack += [autobound(item, context)]
      "dev" => {
        var index = 0
        for device in context.devices {
          if device.name == cond.text { index = device.index }
        }
        stack += [index != 0 and item.ifindex == index]
      }
      "fwmark" => stack += [(item.mark ?? 0).bit_and(cond.mask) == cond.mark.bit_and(cond.mask)]
      "cgroup" => stack += [item.cgroup_id != null and (item.cgroup_id ?? -1) == (context.cgroups.get(cond.text) ?? -2)]
      else => stack += [true]
    }
  }
  if stack.is_empty() { return true }
  stack[-1]
}

# Reads the first name per key of a `name key ...` database file: `key_field`
# picks the column that identifies an entry. A missing file leaves the map
# empty, which prints numbers.
proc load_database(database: Str, key_field: Int) [fs, env, error] -> Result[Map[Str, Str], Error] {
  var found: Map[Str, Str] = {}
  let source = accounts.database_file(database)?
  if let Ok(content) = source.read_text() {
    for raw in content.lines() {
      var line = raw
      let comment = raw.find("#")
      if comment != null { line = raw.byte_slice(0, length: comment) }
      let fields = line.fields()
      if fields.len() < 2 { continue }
      if found.get(fields[key_field]) is Err(_) {
        found[fields[key_field]] = fields[if key_field == 0 { 1 } else { 0 }]
      }
    }
  }
  Ok(found)
}

proc read_names(config: Config) [fs, env, error] -> Names {
  var services: Map[Str, Str] = {}
  var protocols: Map[Str, Str] = {}
  var hosts: Map[Str, Str] = {}
  # Filters accept service names even with -n, which only keeps the printed
  # ports numeric.
  services = load_database("services", 1)?
  protocols = load_database("protocols", 1)?
  if config.resolve { hosts = load_database("hosts", 0)? }
  {services: services, protocols: protocols, hosts: hosts}
}

proc host_text(address: Str, names: Names) [env, net, error] -> Str {
  if let Ok(known) = names.hosts.get(address) { return known }
  # A fixture hosts file stands alone: the resolver is only consulted when
  # the default database is in use.
  if e"XSH_HOSTS_FILE" is Err(_) {
    if let Ok(found) = dns.reverse(address) {
      if !found.is_empty() { return found[0] }
    }
  }
  address
}

pure port_text(port: Int, table: Str, config: Config, names: Names) -> Str {
  if port == 0 { return "*" }
  if config.numeric { return f"{port}" }
  if table == "raw" {
    return names.protocols.get(f"{port}") ?? f"ipproto-{port}"
  }
  names.services.get(f"{port}/{table}") ?? f"{port}"
}

# Blanks for padding; widths are byte counts, as in the reference tool.
pure spaces(count: Int) -> Str {
  var out = ""
  var left = count
  while left > 0 {
    let chunk = if left > 16 { 16 } else { left }
    out += "                ".byte_slice(0, length: chunk)
    left -= chunk
  }
  out
}

const HEADERS = ["Netid", "State", "Recv-Q", "Send-Q", "Local Address:", "Port", "Peer Address:", "Port", "Process", ""]
const DELIMS = ["", " ", " ", " ", " ", "", " ", "", "", ""]

proc endpoint_cell(item: sockets.DiagSocket, remote: Bool, config: Config, names: Names, devices: List[Device]) [env, net, error] -> List[Str] {
  let table = item.netid
  if item.family == sockets.AF_UNIX {
    let name = if remote { "*" } else { item.name ?? "*" }
    let port = if remote { item.remote_port } else { item.local_port }
    return [name + " ", f"{port}"]
  }
  let data = if remote { item.remote } else { item.local }
  let port = if remote { item.remote_port } else { item.local_port }
  let v6only = (item.v6only ?? 0) != 0
  var text = sockets.endpoint_address(data, item.family, v6only)
  var zero = true
  for index in range(data.len()) {
    if data.byte_at(index) != 0 { zero = false }
  }
  if config.resolve and !zero and text != "*" { text = host_text(text, names) }
  if !remote and item.ifindex != 0 {
    for device in devices {
      if device.index == item.ifindex { text += "%" + device.name }
    }
  }
  if text.find(":") != null { text = f"[{text}]" }
  [text + ":", port_text(port, table, config, names)]
}

proc socket_row(item: sockets.DiagSocket, config: Config, names: Names, owners: List[sockets.Owner], devices: List[Device], cgroup_paths: Map[Str, Str]) [env, net, error] -> List[Str] {
  let local = endpoint_cell(item, false, config, names, devices)
  let remote = endpoint_cell(item, true, config, names, devices)
  var process_text = ""
  if config.processes {
    if let found = sockets.owners_text(owners, item.inode) { process_text = " " + found }
  }
  var ext = ""
  let separator = if config.oneline { "" } else { "\n\t" }
  if !item.detailed {
    ext = ""
  } else if item.family == sockets.AF_UNIX {
    if config.memory {
      if let data = item.memory { ext += " " + sockets.skmem_text(data) }
    }
    if config.extended {
      if let mask = item.shutdown {
        ext += f" {if mask.bit_and(1) != 0 { "-" } else { "<" }}-{if mask.bit_and(2) != 0 { "-" } else { ">" }}"
      }
      if let vfs = item.vfs {
        let major = vfs.device / 256 % 4096
        let minor = vfs.device % 256 + vfs.device / 4096 / 256 * 256
        ext += f" ino:{vfs.inode} dev:{major}/{minor}"
      }
      if !item.pending.is_empty() {
        ext += " peers:"
        for peer in item.pending { ext += f" {peer}" }
      }
    }
  } else {
    # -e shows the timer as well, as -o does.
    if config.timers or config.extended {
      if let timer = sockets.timer_text(item.timer, item.expires, item.retrans) { ext += " " + timer }
    }
    if config.extended {
      if item.uid != 0 { ext += f" uid:{item.uid}" }
      ext += f" ino:{item.inode} sk:{item.cookie}"
      if let mark = item.mark { if mark != 0 { ext += f" fwmark:0x{sockets.hex(mark)}" } }
      if let id = item.cgroup_id {
        ext += " cgroup:" + (cgroup_paths.get(f"{id}") ?? f"unreachable:{sockets.hex(id)}")
      }
      if item.family == sockets.AF_INET6 {
        if let only = item.v6only { ext += f" v6only:{only}" }
      }
      if let mask = item.shutdown {
        ext += f" {if mask.bit_and(1) != 0 { "-" } else { "<" }}-{if mask.bit_and(2) != 0 { "-" } else { ">" }}"
      }
    }
    let tcp_like = item.netid != "udp"
    if config.memory or (config.info and tcp_like) {
      ext += separator
      if config.memory {
        if let data = item.memory { ext += " " + sockets.skmem_text(data) }
      }
      if config.info and tcp_like {
        let text = sockets.tcp_info_text(item.info ?? b"", item.congestion, item.state, config.timers or config.extended)
        if text != "" { ext += " " + text }
      }
    }
  }
  [item.netid, sockets.state_name(item.state), f"{item.recv_queue}", f"{item.send_queue}", local[0], local[1], remote[0], remote[1], process_text, ext]
}

proc render(rows: List[List[Str]], visible: List[Bool], config: Config) [process, env, io] {
  var widths: List[Int] = []
  for column in range(10) {
    var width = 0
    if !config.no_header { width = HEADERS[column].byte_len() }
    if column == 2 or column == 3 { width = 6 }
    for row in rows {
      let size = row[column].byte_len()
      if size > width { width = size }
    }
    widths += [width]
  }
  # The last visible table column (Process, or the peer port without -p) is
  # not padded, so rows carry no trailing blanks there; the extra-text column
  # after it is always padded to its widest cell.
  var last_table = 0
  for column in range(9) {
    if visible[column] { last_table = column }
  }
  var out = ""
  var lines = rows
  if !config.no_header { lines = [HEADERS].extend(rows) }
  for row in lines {
    var first = true
    var text = ""
    for column in range(10) {
      if !visible[column] { continue }
      if !first { text += DELIMS[column] }
      first = false
      let cell = row[column]
      if column == last_table or column == 8 {
        text += cell
      } else if column == 4 or column == 6 {
        text += spaces(widths[column] - cell.byte_len()) + cell
      } else {
        text += cell + spaces(widths[column] - cell.byte_len())
      }
    }
    out += text + "\n"
  }
  gnu.write_text(out)
}

proc print_summary() [fs, process, env, io, error] {
  let counts = match sockets.summary() {
    Ok(counts) => counts
    Err(failure) => { fail(failure.message, 1); return }
  }
  let ip4 = counts.raw4 + counts.udp4 + counts.tcp4
  let ip6 = counts.raw6 + counts.udp6 + counts.tcp6
  let waiting = counts.time_wait
  let allocated = counts.allocated
  var out = f"Total: {counts.used}\n"
  out += f"TCP:   {allocated + waiting} (estab {counts.established}, closed {allocated + waiting - counts.tcp4 - counts.tcp6}, orphaned {counts.orphaned}, timewait {waiting})\n\n"
  out += "Transport Total     IP        IPv6\n"
  out += summary_line("RAW", counts.raw4 + counts.raw6, counts.raw4, counts.raw6)
  out += summary_line("UDP", counts.udp4 + counts.udp6, counts.udp4, counts.udp6)
  out += summary_line("TCP", counts.tcp4 + counts.tcp6, counts.tcp4, counts.tcp6)
  out += summary_line("INET", ip4 + ip6, ip4, ip6)
  out += summary_line("FRAG", counts.frag4 + counts.frag6, counts.frag4, counts.frag6)
  out += "\n"
  gnu.write_text(out)
}

pure summary_line(name: Str, total: Int, four: Int, six: Int) -> Str {
  f"{name}\t  {f"{total}" + spaces(9 - f"{total}".byte_len())} {f"{four}" + spaces(9 - f"{four}".byte_len())} {f"{six}" + spaces(9 - f"{six}".byte_len())}\n"
}

proc main(...argv: List[Str]) [fs, process, env, io, net, error] {
  let config = parse_args(argv)
  if config.help {
    gnu.help(USAGE)
    return
  }
  if config.version {
    gnu.write_text("ss utility, XSH core\n")
    return
  }
  if config.context {
    if selinux.mounted() {
      fail("SELinux contexts are not supported", 1)
    }
    fail("SELinux is not enabled.", 1)
  }

  # Which tables to dump: explicit ones, or all of them. An address-family
  # option without an explicit inet table selects the inet tables.
  let inet_tables = ["raw", "udp", "tcp"]
  var tables = config.tables
  let family_is_inet = config.family == sockets.AF_INET or config.family == sockets.AF_INET6
  var has_inet = false
  for table in tables {
    if table in inet_tables { has_inet = true }
  }
  if tables.is_empty() {
    tables = if family_is_inet { inet_tables } else { ["u_str", "u_dgr", "u_seq", "raw", "udp", "tcp"] }
  } else if family_is_inet and !has_inet {
    tables = tables.extend(inet_tables)
  }

  # State filter clauses lead the filter words.
  var states = 0
  if config.state_mode == "all" { states = ALL_STATES }
  if config.state_mode == "listening" { states = LISTEN_BIT + CLOSE_BIT }
  # The reference tool joins its arguments, so one quoted argument may hold
  # a whole expression.
  var words = config.words.join(" ").fields()
  var saw_states = false
  while !words.is_empty() and words[0] in ["state", "exclude", "excl"] {
    let keyword = words[0]
    if words.len() < 2 {
      eprint "Command line is not complete. Try option \"help\""
      exit 255
    }
    let mask = scan_state(words[1])
    if mask < 0 { fail(f"wrong state name: {words[1]}", 255) }
    if keyword == "state" {
      if !saw_states { states = 0 }
      states = states.bit_or(mask)
    } else {
      if !saw_states { states = ALL_STATES }
      states = states.clear_bits(mask)
    }
    saw_states = true
    words = words |> drop(2)
  }
  let show_state_column = popcount(states) != 1
  if states == 0 {
    states = ALL_STATES - LISTEN_BIT - CLOSE_BIT - state_bit(sockets.TCP_TIME_WAIT) - state_bit(sockets.TCP_SYN_RECV) - 1
  }
  # Bit 0 is the unknown state and is never part of a request.
  states = states.bit_and(4094)

  let names = read_names(config)
  var program: List[Cond] = []
  if !words.is_empty() {
    let parsed = parse_or(words, 0, config.family, tables, names)
    if parsed.position != words.len() { syntax_error() }
    program = parsed.output
  }

  let channel = sockets.open() ?? { |failure|
    fail(f"cannot open the sock_diag netlink socket: {gnu.strerror(failure)}", 1)
    0
  }
  defer unix.close_fd(channel)

  # -s with no table, family or filter selection prints only the summary.
  if config.summary {
    print_summary()
    if config.tables.is_empty() and config.family == 0 and words.is_empty() { return }
  }

  var devices: List[Device] = []
  var needs_devices = false
  for cond in program {
    if cond.kind == "dev" { needs_devices = true }
  }
  if needs_devices {
    let dump = linux.network_dump()?
    for link in dump.links {
      if let name = link.name { devices += [{index: link.ifindex, name: name}] }
    }
    for cond in program {
      if cond.kind == "dev" {
        var known = false
        for device in devices {
          if device.name == cond.text { known = true }
        }
        if !known {
          eprint "Cannot parse device."
          exit 1
        }
      }
    }
  }
  var group_ids: Map[Str, Int] = {}
  var any_cgroup = false
  for cond in program {
    if cond.kind == "cgroup" {
      any_cgroup = true
      if let id = sockets.cgroup_id(cond.text) {
        group_ids[cond.text] = id
      } else {
        eprint "Invalid cgroup2 path"
        eprint f"Cannot parse cgroup {cond.text}."
        exit 1
      }
    }
  }
  let ports = sockets.local_port_range()
  let context: Context = {low: ports.low, high: ports.high, devices: devices, cgroups: group_ids}

  let want_info = config.info
  var found: List[sockets.DiagSocket] = []
  var incomplete = false

  var unix_tables: List[Str] = []
  for table in ["u_str", "u_dgr", "u_seq"] {
    if table in tables { unix_tables += [table] }
  }
  if !unix_tables.is_empty() {
    let listed = sockets.collect_unix(channel, states, config.memory, config.extended)
    if let Ok(list) = listed {
      for item in list {
        if item.netid in unix_tables { found += [item] }
      }
    } else if let Err(failure) = listed {
      eprint f"ss: cannot list unix sockets: {gnu.strerror(failure)}"
      incomplete = true
    }
  }
  for table in inet_tables {
    if table not in tables { continue }
    for family in [sockets.AF_INET, sockets.AF_INET6] {
      if family_is_inet and config.family != family { continue }
      let listed = sockets.collect_inet(channel, family, sockets.protocol_number(table), states, config.memory, want_info)
      if let Ok(list) = listed {
        for item in list {
          if item.netid == table { found += [item] }
        }
      } else if let Err(failure) = listed {
        eprint f"ss: cannot list {table} sockets: {gnu.strerror(failure)}"
        incomplete = true
      }
    }
  }

  let owners = if config.processes { sockets.owners()? } else { [] }
  var cgroup_map: Map[Str, Str] = {}
  if config.extended and !found.is_empty() { cgroup_map = sockets.cgroup_names() }
  var interfaces: List[Device] = devices
  if interfaces.is_empty() {
    var bound = false
    for item in found {
      if item.ifindex != 0 { bound = true }
    }
    if bound {
      if let Ok(dump) = linux.network_dump() {
        for link in dump.links {
          if let name = link.name { interfaces += [{index: link.ifindex, name: name}] }
        }
      }
    }
  }
  var rows: List[List[Str]] = []
  for item in found {
    if !program.is_empty() and !evaluate(program, item, context) { continue }
    rows += [socket_row(item, config, names, owners, interfaces, cgroup_map)]
  }

  var visible: List[Bool] = []
  for column in range(10) {
    var shown = true
    if column == 0 and tables.len() == 1 { shown = false }
    if column == 1 and !show_state_column { shown = false }
    if (column == 2 or column == 3) and config.no_queues { shown = false }
    if column == 8 and !config.processes { shown = false }
    visible += [shown]
  }
  # A one-line listing joins the extra lines of each socket to its row.
  render(rows, visible, config)
  if incomplete { exit 1 }
}
