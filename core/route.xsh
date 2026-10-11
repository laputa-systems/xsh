#!/bin/xsh
use lib.gnu
use lib.nettools

const USAGE = """Usage: route [-nNvee] [-FC] [<AF>]           List kernel routing tables
       route [-v] [-FC] {add|del} ...        Modify routing table for AF.

       route {-h|--help}                     Detailed usage syntax.
       route {-V|--version}                  Display version/author and exit.

        -v, --verbose            be verbose
        -n, --numeric            don't resolve names
        -N, --symbolic           resolve hardware names
        -e, --extend             display other/more information
        -F, --fib                display Forwarding Information Base (default)
        -C, --cache              display routing cache instead of FIB

  <AF>=Use -4, -6, '-A <af>' or '--<af>'; default: inet
  List of possible address families (which support routing):
    inet (DARPA Internet) inet6 (IPv6)
"""

const INET_USAGE = """Usage: inet_route [-vF] del {-host|-net} Target[/prefix] [gw Gw] [metric M] [[dev] If]
       inet_route [-vF] add {-host|-net} Target[/prefix] [gw Gw] [metric M]
                              [netmask N] [mss Mss] [window W] [irtt I]
                              [[dev] If]
       inet_route [-vF] add {-host|-net} Target[/prefix] [metric M] reject
       inet_route [-FC] flush      NOT supported
"""

const INET6_USAGE = """Usage: inet6_route [-vF] del Target
       inet6_route [-vF] add Target [gw Gw] [metric M] [[dev] If]
       inet6_route [-FC] flush      NOT supported
"""

type Split = {text: Str, prefix: Int?}

type Request = {
  add: Bool, family: Str, network: Bool?, target: Str, netmask: Str?, gateway: Str?, metric: Int?,
  mss: Int?, window: Int?, irtt: Int?, reject: Bool, device: Str?,
}

# Everything after the verb is a modify request, which has its own usage text
# and exits with the legacy status 3 when it cannot be understood.
proc modify_usage(family: Str) [process, error] {
  let text = if family == "inet6" { INET6_USAGE } else { INET_USAGE }
  eprint nettools.chomp(text)
  exit 3
}

# A name in the hosts file, forward lookup only.
proc resolve_host(word: Str) [fs, process, error] -> Bytes {
  if let raw = nettools.ipv4_parse(word) { return raw }
  if let address = nettools.hosts_address(word) {
    if let raw = nettools.ipv4_parse(address) { return raw }
  }
  eprint f"{word}: Unknown host"
  exit 6
  b""
}

proc networks_names() [fs] -> Map[Str, Str] {
  var names: Map[Str, Str] = {}
  let text = fp"/etc/networks".read_text() ?? ""
  for line in text.lines() {
    let words = line.split("#")[0].words()
    continue when words.len() < 2
    let parts = words[1].split(".")
    var octets: List[Str] = parts
    while octets.len() < 4 { octets += ["0"] }
    if octets.len() == 4 and words[1] not in names.keys() { names = names.set(octets.join("."), words[0]) }
  }
  names
}

proc ipv4_listing(numeric: Bool, extend: Int, cache: Bool) [fs, process, env, io, error] {
  if cache {
    gnu.write_text(nettools.ROUTE4_CACHE_HEADER + "\n")
    return
  }
  let rows = nettools.ipv4_routes()?
  let header = if extend == 0 {
    nettools.ROUTE4_HEADER
  } else if extend == 1 {
    nettools.ROUTE4_EXTENDED_HEADER
  } else {
    nettools.ROUTE4_EXTRA_HEADER
  }
  gnu.write_text(header + "\n")
  var hosts: Map[Str, Str] = {}
  var networks: Map[Str, Str] = {}
  if !numeric {
    hosts = nettools.hosts_names()
    networks = networks_names()
  }
  for row in rows {
    var destination = row.destination
    var gateway = row.gateway
    if !numeric {
      if row.destination == "0.0.0.0" and row.prefix == 0 {
        destination = "default"
      } else if row.prefix == 32 {
        destination = hosts.get(row.destination) ?? networks.get(row.destination) ?? row.destination
      } else {
        destination = networks.get(row.destination) ?? hosts.get(row.destination) ?? row.destination
      }
      gateway = hosts.get(row.gateway) ?? row.gateway
    }
    gnu.write_text(nettools.route4_row(row, destination, gateway, extend) + "\n")
  }
}

proc ipv6_listing(numeric: Bool, cache: Bool) [fs, process, env, io, error] {
  let rows = nettools.ipv6_routes()?
  let header = if cache { "Kernel IPv6 routing cache\n" + nettools.ROUTE6_HEADER.split("\n")[1] } else { nettools.ROUTE6_HEADER }
  gnu.write_text(header + "\n")
  var hosts: Map[Str, Str] = {}
  if !numeric { hosts = nettools.hosts_names() }
  for row in rows {
    let is_cache = row.flags.bit_and(16777216) != 0
    continue when is_cache != cache
    var destination = f"{row.destination}/{row.prefix}"
    var nexthop = row.nexthop
    if !numeric {
      let shown = if row.destination == "::" { "[::]" } else { hosts.get(row.destination) ?? row.destination }
      destination = f"{shown}/{row.prefix}"
      if row.nexthop == "::" { nexthop = "[::]" } else { nexthop = hosts.get(row.nexthop) ?? row.nexthop }
    }
    gnu.write_text(nettools.route6_row(row, destination, nexthop) + "\n")
  }
}

# Splits `ADDR[/PREFIX]`; a malformed prefix is a usage error.
proc split_prefix(word: Str, limit: Int, family: Str) [process, error] -> Split {
  let slash = word.find("/")
  if slash == null { return {text: word, prefix: null} }
  let parsed = nettools.decimal(word.byte_slice((slash ?? 0) + 1))
  if parsed == null or (parsed ?? 0) > limit { modify_usage(family) }
  {text: word.byte_slice(0, length: slash ?? 0), prefix: parsed}
}

pure int_attribute(kind: Int, value: Int) -> Bytes {
  bytes.concat([bytes.pack_le(8, 2) ?? b"", bytes.pack_le(kind, 2) ?? b"", bytes.pack_le(value, 4) ?? b""])
}

pure raw_attribute(kind: Int, data: Bytes) -> Bytes {
  let length = 4 + data.len()
  let padding = (4 - length % 4) % 4
  bytes.concat([bytes.pack_le(length, 2) ?? b"", bytes.pack_le(kind, 2) ?? b"", data, bytes.zero(padding) ?? b""])
}

proc device_index(name: Str, label: Str) [process, net, env, error] -> Int {
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  let request = nettools.ifreq(name, b"") ?? b""
  match linux.ioctl(socket, c.SIOCGIFINDEX, request, 40) {
    Ok(reply) => bytes.unpack_le(reply, 4, 16) ?? 0
    Err(failure) => {
      eprint f"{label}: {gnu.strerror(failure)}"
      exit 7
      0
    }
  }
}

# Sends one route-netlink request built from a parsed modify request. The
# kernel applies the same validation the ioctl path would, so its errno
# decides the message.
proc send_route(request: Request) [fs, process, net, env, error] {
  let c = linux.net_constants()
  let label = if request.add { "SIOCADDRT" } else { "SIOCDELRT" }
  let inet6 = request.family == "inet6"
  var destination = b""
  var prefix = 0
  var gateway: Bytes? = null
  if inet6 {
    let split = split_prefix(request.target, 128, "inet6")
    if request.target == "default" {
      destination = bytes.zero(16)?
    } else {
      let raw = nettools.ipv6_parse(split.text)
      if raw == null {
        eprint f"{request.target}: Unknown host"
        exit 6
      }
      destination = raw ?? b""
      prefix = split.prefix ?? 128
    }
    if let word = request.gateway {
      let raw = nettools.ipv6_parse(word)
      if raw == null {
        eprint f"{word}: Unknown host"
        exit 6
      }
      gateway = raw
    }
  } else {
    var network = request.network
    var target = request.target
    var given_prefix: Int? = null
    if target == "default" {
      destination = bytes.zero(4)?
      network = true
    } else {
      let split = split_prefix(target, 32, "inet")
      target = split.text
      given_prefix = split.prefix
      destination = resolve_host(target)
    }
    if given_prefix != null and request.netmask != null { modify_usage("inet") }
    var mask_bytes: Bytes? = null
    if let length = given_prefix { mask_bytes = nettools.prefix_mask(length) }
    if let word = request.netmask {
      mask_bytes = nettools.netmask_parse(word)
      if mask_bytes == null {
        eprint f"{word}: Unknown host"
        exit 6
      }
    }
    let is_net = network == true
    if is_net and mask_bytes != null {
        # An explicit mask must leave no host bits set in the destination.
        let given = mask_bytes ?? b""
        for index in range(4) {
          if (destination.byte_at(index) ?? 0).bit_and(255 - (given.byte_at(index) ?? 0)) != 0 {
            eprint "route: netmask doesn't match route address"
            modify_usage("inet")
          }
        }
    }
    if is_net {
      let length = nettools.mask_prefix(mask_bytes ?? bytes.zero(4)?)
      if length == null {
        eprint f"route: netmask {request.netmask ?? ""} is not a contiguous mask"
        modify_usage("inet")
      }
      prefix = length ?? 0
    } else {
      if mask_bytes != null and mask_bytes != nettools.prefix_mask(32) {
        # The legacy message prints the complement of the mask in host order.
        let shown = mask_bytes ?? b""
        let value = 255 - (shown.byte_at(0) ?? 0)
        let wildcard = value * 16777216 + (255 - (shown.byte_at(1) ?? 0)) * 65536 + (255 - (shown.byte_at(2) ?? 0)) * 256 + (255 - (shown.byte_at(3) ?? 0))
        eprint f"route: netmask {hex8(wildcard)} doesn't make sense with host route"
        modify_usage("inet")
      }
      prefix = 32
    }
    if let word = request.gateway { gateway = resolve_host(word) }
  }

  var attributes: List[Bytes] = [raw_attribute(1, destination)]
  var scope = if inet6 { 0 } else if gateway == null { 253 } else { 0 }
  var kind = 1
  var protocol = 3
  if request.reject {
    kind = 7
    scope = 254
  } else {
    if let raw = gateway { attributes += [raw_attribute(5, raw)] }
    if let name = request.device { attributes += [int_attribute(4, device_index(name, label))] }
  }
  # A delete matches on destination, nexthop, and priority only: scope, type,
  # and protocol are wildcards, as they are for the legacy ioctl.
  if !request.add {
    scope = 255
    kind = 0
    protocol = 0
  }
  var priority = request.metric ?? 0
  if inet6 and request.metric == null { priority = 1 }
  if priority > 0 { attributes += [int_attribute(6, priority)] }
  var metrics: List[Bytes] = []
  if let mss = request.mss { metrics += [int_attribute(8, mss - 40)] }
  if let window = request.window { metrics += [int_attribute(3, window)] }
  if let irtt = request.irtt { metrics += [int_attribute(4, irtt * 8)] }
  if !metrics.is_empty() { attributes += [raw_attribute(8, bytes.concat(metrics))] }

  let family = if inet6 { c.AF_INET6 } else { c.AF_INET }
  let header = bytes.from_ints([family, prefix, 0, 0, 254, protocol, scope, kind])?
  let payload = bytes.concat([header, bytes.zero(4)?, bytes.concat(attributes)])
  # CREATE alone (no EXCL, no APPEND) is what the legacy ioctl path asks for: a
  # route with the same destination and metric over another nexthop or device
  # is listed ahead of the old one, and only an identical route is refused with
  # "File exists".
  let flags = if request.add {
    c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK).bit_or(c.NLM_F_CREATE)
  } else {
    c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK)
  }
  let channel = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(channel)
  match linux.netlink_request(channel, if request.add { c.RTM_NEWROUTE } else { c.RTM_DELROUTE }, flags, payload) {
    Ok(_) => {}
    Err(failure) => {
      eprint f"{label}: {gnu.strerror(failure)}"
      exit 7
    }
  }
}

pure hex8(value: Int) -> Str {
  var out = ""
  var rest = value
  for _ in range(8) {
    out = "0123456789abcdef".byte_slice(rest % 16, length: 1) + out
    rest = rest / 16
  }
  out
}

# Parses the words after add or del into a request.
proc parse_modify(add: Bool, family: Str, words: List[Str]) [process, error] -> Request {
  var at = 0
  var network: Bool? = null
  if at < words.len() and words[at] == "-net" {
    network = true
    at += 1
  } else if at < words.len() and words[at] == "-host" {
    network = false
    at += 1
  }
  if family == "inet6" and network != null { modify_usage(family) }
  if at >= words.len() { modify_usage(family) }
  var request: Request = {
    add: add, family: family, network: network, target: words[at], netmask: null, gateway: null, metric: null,
    mss: null, window: null, irtt: null, reject: false, device: null,
  }
  at += 1
  while at < words.len() {
    let word = words[at]
    at += 1
    let has_value = at < words.len()
    match word {
      "netmask" => {
        if family == "inet6" or !has_value or request.netmask != null { modify_usage(family) }
        request = {...request, netmask: words[at]}
        at += 1
      }
      "gw" => {
        if !has_value or request.gateway != null { modify_usage(family) }
        request = {...request, gateway: words[at]}
        at += 1
      }
      "metric" => {
        if !has_value { modify_usage(family) }
        let value = nettools.decimal(words[at])
        if value == null or (value ?? 0) > 65535 { modify_usage(family) }
        request = {...request, metric: value}
        at += 1
      }
      "mss" | "window" | "irtt" => {
        if family == "inet6" or !has_value { modify_usage(family) }
        let value = nettools.decimal(words[at])
        at += 1
        if word == "mss" {
          if value == null or (value ?? 0) < 64 or (value ?? 0) > 32768 {
            eprint "route: Invalid MSS/MTU."
            exit 3
          }
          request = {...request, mss: value}
        } else if word == "window" {
          if value == null or (value ?? 0) < 1 or (value ?? 0) > 65535 {
            eprint "route: Invalid window size."
            exit 3
          }
          request = {...request, window: value}
        } else {
          if value == null or (value ?? 0) < 1 or (value ?? 0) > 65535 {
            eprint "route: Invalid initial rtt."
            exit 3
          }
          request = {...request, irtt: value}
        }
      }
      "reject" => request = {...request, reject: true}
      "mod" | "dyn" | "reinstate" => {
        eprint f"route: `{word}' is accepted by net-tools but has no effect on Linux; not supported"
        exit 3
      }
      "dev" => {
        if !has_value or request.device != null { modify_usage(family) }
        request = {...request, device: words[at]}
        at += 1
      }
      else => {
        # A bare device name is only valid as the last word.
        if at < words.len() or request.device != null { modify_usage(family) }
        request = {...request, device: word}
      }
    }
  }
  request
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  var numeric = false
  var extend = 0
  var cache = false
  var family = "inet"
  var at = 0
  while at < argv.len() and argv[at].starts_with("-") and argv[at] != "-net" and argv[at] != "-host" {
    let word = argv[at]
    at += 1
    if word.starts_with("--") {
      match word {
        "--numeric" => numeric = true
        "--symbolic" => numeric = false
        "--verbose" => {}
        "--extend" => extend += 1
        "--fib" => cache = false
        "--cache" => cache = true
        "--inet" => family = "inet"
        "--inet6" => family = "inet6"
        "--version" => {
          gnu.version("route")
          return
        }
        "--help" => {
          gnu.help(USAGE)
          return
        }
        else => {
          eprint f"route: unrecognized option: {word}"
          eprint nettools.chomp(USAGE)
          exit 3
        }
      }
      continue
    }
    let letters = word.byte_slice(1).split("")
    var position = 0
    while position < letters.len() {
      let letter = letters[position]
      position += 1
      match letter {
        "n" => numeric = true
        "N" => numeric = false
        # The legacy tool prints nothing extra for -v on the paths this applet
        # takes, so the flag is accepted for scripts that pass it.
        "v" => {}
        "e" => extend += 1
        "F" => cache = false
        "C" => cache = true
        "4" => family = "inet"
        "6" => family = "inet6"
        "V" => {
          gnu.version("route")
          return
        }
        "h" | "?" => {
          gnu.help(USAGE)
          return
        }
        "A" => {
          # The family is the rest of the cluster (-Ainet6) or the next word.
          var given = ""
          if position < letters.len() {
            given = letters[position..].join("")
            position = letters.len()
          } else {
            if at >= argv.len() {
              eprint nettools.chomp(USAGE)
              exit 3
            }
            given = argv[at]
            at += 1
          }
          if given not in ["inet", "inet6"] {
            eprint f"Unknown address family `{given}'."
            exit 1
          }
          family = given
        }
        else => {
          eprint f"route: unrecognized option: {letter}"
          eprint nettools.chomp(USAGE)
          exit 3
        }
      }
    }
  }
  if at < argv.len() and argv[at] in ["inet", "inet6"] {
    family = argv[at]
    at += 1
  }
  if at >= argv.len() {
    if family == "inet6" { ipv6_listing(numeric, cache) } else { ipv4_listing(numeric, extend, cache) }
    return
  }
  let verb = argv[at]
  let words = argv[at + 1..]
  match verb {
    "add" => send_route(parse_modify(true, family, words))
    "del" | "delete" => send_route(parse_modify(false, family, words))
    "flush" => {
      eprint f"Flushing `{family}' routing table not supported"
      modify_usage(family)
    }
    else => {
      eprint nettools.chomp(USAGE)
      exit 3
    }
  }
}
