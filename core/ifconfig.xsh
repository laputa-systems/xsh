#!/bin/xsh
use lib.gnu
use lib.nettools

const USAGE = """Usage:
  ifconfig [-a] [-v] [-s] <interface> [[<AF>] <address>]
  [add <address>[/<prefixlen>]]
  [del <address>[/<prefixlen>]]
  [[-]broadcast [<address>]]  [[-]pointopoint [<address>]]
  [netmask <address>]  [dstaddr <address>]
  [hw ether <address>]  [mtu <NN>]
  [[-]trailers]  [[-]arp]  [[-]allmulti]
  [multicast]  [[-]promisc]
  [txqueuelen <NN>]
  [name <newname>]
  [[-]dynamic]
  [up|down] ...

  <AF>=Address family. Default: inet
  List of possible address families:
    inet (DARPA Internet) inet6 (IPv6)
  <HW>=Hardware Type.
  List of possible hardware types:
    ether (Ethernet)
"""

# Words that net-tools accepts and that need a device-private or map ioctl this
# applet does not issue; each is refused by name instead of being ignored.
const UNSUPPORTED = ["mem_start", "io_addr", "irq", "media", "outfill", "keepalive", "tunnel"]

const HELP_HINT = "ifconfig: `--help' gives usage information."

proc usage_error() [process] {
  eprint nettools.chomp(USAGE)
  exit 3
}

proc bad_address(word: Str) [process, error] {
  eprint f"{word}: Unknown host"
  eprint $HELP_HINT
  exit 1
}

# Resolves an IPv4 operand: a dotted quad, or a name in the hosts file.
proc resolve_inet(word: Str) [fs, process, error] -> Bytes {
  if let raw = nettools.ipv4_parse(word) { return raw }
  if let address = nettools.hosts_address(word) {
    if let raw = nettools.ipv4_parse(address) { return raw }
  }
  bad_address(word)
  b""
}

# A `struct sockaddr_in` as it sits inside an ifreq: family, port, address.
pure sockaddr_in(raw: Bytes) -> Bytes {
  bytes.concat([bytes.from_ints([2, 0, 0, 0]) ?? b"", raw, bytes.zero(8) ?? b""])
}

# Folds request statuses the way net-tools ORs them: any -1 stays -1.
pure combine(left: Int, right: Int) -> Int {
  if left < 0 or right < 0 { return -1 }
  if left != 0 or right != 0 { return 1 }
  0
}

proc report(label: Str, failure: Error) [process, env] {
  eprint f"{label}: {gnu.strerror(failure)}"
}

# Reads, changes, and writes back the interface flags the way net-tools'
# set_flag and clr_flag do. Returns the status the applet folds into its own.
proc change_flags(fd: Int, name: Str, set_bits: Int, clear_bits: Int) [process, env, error] -> Int {
  let c = linux.net_constants()
  let request = nettools.ifreq(name, b"")?
  var current = 0
  match linux.ioctl(fd, c.SIOCGIFFLAGS, request, 40) {
    Ok(reply) => current = bytes.unpack_le(reply, 2, 16) ?? 0
    Err(failure) => {
      eprint f"{name}: ERROR while getting interface flags: {gnu.strerror(failure)}"
      return -1
    }
  }
  let wanted = current.bit_or(set_bits).clear_bits(clear_bits)
  let update = nettools.ifreq(name, bytes.pack_le(wanted, 2)?)?
  match linux.ioctl(fd, c.SIOCSIFFLAGS, update, 0) {
    Ok(_) => 0
    Err(failure) => {
      eprint f"{name}: ERROR while setting interface flags: {gnu.strerror(failure)}"
      -1
    }
  }
}

# Issues one fixed-size interface ioctl and reports a failure as `LABEL: ...`.
proc interface_ioctl(fd: Int, label: Str, request: Int, name: Str, tail: Bytes) [process, env, error] -> Int {
  let payload = nettools.ifreq(name, tail)?
  match linux.ioctl(fd, request, payload, 0) {
    Ok(_) => 0
    Err(failure) => {
      report(label, failure)
      1
    }
  }
}

# Parses a hardware address of exactly six hexadecimal octets.
# Adds or deletes one IPv6 address; the ifreq ioctls only add, so both go
# through route netlink. Returns the status for the applet.
proc change_inet6(name: Str, word: Str, add: Bool) [fs, process, net, env, error] -> Int {
  let c = linux.net_constants()
  var text = word
  var prefix = 128
  let slash = word.find("/")
  if slash != null {
    text = word.byte_slice(0, length: slash ?? 0)
    let parsed = nettools.decimal(word.byte_slice((slash ?? 0) + 1))
    if parsed == null or (parsed ?? 0) > 128 { usage_error() }
    prefix = parsed ?? 128
  }
  let raw = nettools.ipv6_parse(text)
  if raw == null { bad_address(word) }
  let address = raw ?? b""
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  let index_reply = match linux.ioctl(socket, c.SIOCGIFINDEX, nettools.ifreq(name, b"")?, 40) {
    Ok(reply) => reply
    Err(failure) => {
      report(if add { "SIOCSIFADDR" } else { "SIOCDIFADDR" }, failure)
      return 1
    }
  }
  let index = bytes.unpack_le(index_reply, 4, 16) ?? 0
  let message = bytes.concat(
    [
      bytes.from_ints([c.AF_INET6, prefix, 0, 0])?,
      bytes.pack_le(index, 4)?,
      bytes.pack_le(20, 2)?,
      bytes.pack_le(2, 2)?,
      address,
    ],
  )
  let channel = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(channel)
  let flags = if add {
    c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK).bit_or(c.NLM_F_CREATE).bit_or(c.NLM_F_EXCL)
  } else {
    c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK)
  }
  match linux.netlink_request(channel, if add { c.RTM_NEWADDR } else { c.RTM_DELADDR }, flags, message) {
    Ok(_) => 0
    Err(failure) => {
      report(if add { "SIOCSIFADDR" } else { "SIOCDIFADDR" }, failure)
      1
    }
  }
}

# Applies the words after the interface name in order, as net-tools does, and
# returns the accumulated status: 1 for a failed request, -1 when the flags of
# the interface cannot be read.
proc configure(name_in: Str, words: List[Str]) [fs, process, net, env, error] -> Int {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)
  var name = name_in
  var status = 0
  var family = "inet"
  var at = 0
  while at < words.len() {
    let word = words[at]
    at += 1
    let has_next = at < words.len()
    match word {
      "up" => status = combine(status, change_flags(fd, name, c.IFF_UP.bit_or(c.IFF_RUNNING), 0))
      "down" => status = combine(status, change_flags(fd, name, 0, c.IFF_UP))
      "arp" => status = combine(status, change_flags(fd, name, 0, c.IFF_NOARP))
      "-arp" => status = combine(status, change_flags(fd, name, c.IFF_NOARP, 0))
      "trailers" => status = combine(status, change_flags(fd, name, 0, c.IFF_NOTRAILERS))
      "-trailers" => status = combine(status, change_flags(fd, name, c.IFF_NOTRAILERS, 0))
      "promisc" => status = combine(status, change_flags(fd, name, c.IFF_PROMISC, 0))
      "-promisc" => status = combine(status, change_flags(fd, name, 0, c.IFF_PROMISC))
      "allmulti" => status = combine(status, change_flags(fd, name, c.IFF_ALLMULTI, 0))
      "-allmulti" => status = combine(status, change_flags(fd, name, 0, c.IFF_ALLMULTI))
      "multicast" => status = combine(status, change_flags(fd, name, c.IFF_MULTICAST, 0))
      "-multicast" => status = combine(status, change_flags(fd, name, 0, c.IFF_MULTICAST))
      "dynamic" => status = combine(status, change_flags(fd, name, nettools.IFF_DYNAMIC, 0))
      "-dynamic" => status = combine(status, change_flags(fd, name, 0, nettools.IFF_DYNAMIC))
      "-pointopoint" => status = combine(status, change_flags(fd, name, 0, c.IFF_POINTOPOINT))
      "-broadcast" => {
        status = combine(status, change_flags(fd, name, 0, c.IFF_BROADCAST))
        # The kernel never clears IFF_BROADCAST; say so as net-tools does.
        let check = linux.ioctl(fd, c.SIOCGIFFLAGS, nettools.ifreq(name, b"")?, 40) ?? b""
        if check.len() >= 18 and (bytes.unpack_le(check, 2, 16) ?? 0).bit_and(c.IFF_BROADCAST) != 0 {
          eprint f"Warning: Interface {name} still in BROADCAST mode."
        }
      }
      "broadcast" => {
        if has_next {
          let raw = resolve_inet(words[at])
          at += 1
          status = combine(status, interface_ioctl(fd, "SIOCSIFBRDADDR", c.SIOCSIFBRDADDR, name, sockaddr_in(raw)))
        } else {
          status = combine(status, change_flags(fd, name, c.IFF_BROADCAST, 0))
        }
      }
      "pointopoint" => {
        if has_next {
          let raw = resolve_inet(words[at])
          at += 1
          let result = interface_ioctl(fd, "SIOCSIFDSTADDR", c.SIOCSIFDSTADDR, name, sockaddr_in(raw))
          status = combine(status, result)
          if result == 0 { status = combine(status, change_flags(fd, name, c.IFF_POINTOPOINT, 0)) }
        } else {
          status = combine(status, change_flags(fd, name, c.IFF_POINTOPOINT, 0))
        }
      }
      "dstaddr" => {
        if !has_next { usage_error() }
        let raw = resolve_inet(words[at])
        at += 1
        status = combine(status, interface_ioctl(fd, "SIOCSIFDSTADDR", c.SIOCSIFDSTADDR, name, sockaddr_in(raw)))
      }
      "netmask" => {
        if !has_next { usage_error() }
        let mask = nettools.netmask_parse(words[at])
        if mask == null { bad_address(words[at]) }
        at += 1
        status = combine(status, interface_ioctl(fd, "SIOCSIFNETMASK", c.SIOCSIFNETMASK, name, sockaddr_in(mask ?? b"")))
      }
      "mtu" | "txqueuelen" => {
        if !has_next { usage_error() }
        let value = nettools.decimal(words[at])
        if value == null {
          eprint f"ifconfig: invalid {word} '{words[at]}'"
          exit 1
        }
        at += 1
        let request = if word == "mtu" { c.SIOCSIFMTU } else { c.SIOCSIFTXQLEN }
        let label = if word == "mtu" { "SIOCSIFMTU" } else { "SIOCSIFTXQLEN" }
        status = combine(status, interface_ioctl(fd, label, request, name, bytes.pack_le(value ?? 0, 4)?))
      }
      "hw" => {
        if at + 1 >= words.len() { usage_error() }
        let class = words[at]
        let text = words[at + 1]
        at += 2
        if class != "ether" {
          eprint f"hw address type `{class}' is not supported; only ether can be set"
          exit 1
        }
        let raw = nettools.ether_parse(text)
        if raw == null {
          eprint f"{text}: invalid ether address."
          exit 1
        }
        let tail = bytes.concat([bytes.pack_le(c.ARPHRD_ETHER, 2)?, raw ?? b""])
        status = combine(status, interface_ioctl(fd, "SIOCSIFHWADDR", c.SIOCSIFHWADDR, name, tail))
      }
      "name" => {
        if !has_next { usage_error() }
        let new_name = words[at]
        at += 1
        if bytes.from_text(new_name).len() > 15 {
          eprint f"ifconfig: {new_name}: interface name too long"
          exit 1
        }
        let raw_name = bytes.from_text(new_name)
        let result = interface_ioctl(fd, "SIOCSIFNAME", c.SIOCSIFNAME, name, bytes.concat([raw_name, bytes.zero(16 - raw_name.len())?]))
        status = combine(status, result)
        if result == 0 { name = new_name }
      }
      "add" | "del" => {
        if !has_next { usage_error() }
        let target = words[at]
        at += 1
        status = combine(status, change_inet6(name, target, word == "add"))
      }
      "inet" => family = "inet"
      "inet6" => family = "inet6"
      else => {
        if word in UNSUPPORTED {
          eprint f"ifconfig: {word} is not supported"
          exit 1
        }
        if family == "inet6" {
          eprint "Don't know how to set addresses for family 10."
          exit 1
        }
        var text = word
        var prefix: Int? = null
        let slash = word.find("/")
        if slash != null {
          text = word.byte_slice(0, length: slash ?? 0)
          prefix = nettools.decimal(word.byte_slice((slash ?? 0) + 1))
          if prefix == null or (prefix ?? 0) > 32 { bad_address(word) }
        }
        let raw = resolve_inet(text)
        let result = interface_ioctl(fd, "SIOCSIFADDR", c.SIOCSIFADDR, name, sockaddr_in(raw))
        status = combine(status, result)
        if result == 0 {
          if let length = prefix {
            status = combine(status, 
              interface_ioctl(fd, "SIOCSIFNETMASK", c.SIOCSIFNETMASK, name, sockaddr_in(nettools.prefix_mask(length))),
            )
          }
          status = combine(status, change_flags(fd, name, c.IFF_UP.bit_or(c.IFF_RUNNING), 0))
        }
      }
    }
  }
  status
}

proc show(items: List[nettools.Interface], all: Bool, short: Bool, only: Str?) [process, env, io] -> Int {
  if short { gnu.write_text(nettools.SHORT_HEADER + "\n") }
  var found = false
  for item in items {
    if let wanted = only {
      continue when item.name != wanted
    } else if !all and item.flags.bit_and(nettools.IFF_UP) == 0 {
      continue
    }
    found = true
    if short {
      gnu.write_text(nettools.short_row(item) + "\n")
    } else {
      gnu.write_text(nettools.long_listing(item))
    }
  }
  if only != null and !found {
    eprint f"{only ?? ""}: error fetching interface information: Device not found"
    return 1
  }
  0
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  var all = false
  var short = false
  var verbose = false
  var at = 0
  while at < argv.len() and argv[at].starts_with("-") {
    let word = argv[at]
    match word {
      "-a" => all = true
      "-v" => verbose = true
      "-s" => short = true
      "-V" | "-version" | "--version" => {
        gnu.version("ifconfig")
        return
      }
      "-?" | "-h" | "-help" | "--help" => {
        gnu.help(USAGE)
        return
      }
      else => {
        eprint f"ifconfig: option `{word}' not recognised."
        eprint $HELP_HINT
        exit 1
      }
    }
    at += 1
  }

  let items = nettools.interfaces() ?? { |failure|
    eprint f"ifconfig: cannot read the interface list: {gnu.strerror(failure)}"
    exit 1
  }
  if at >= argv.len() {
    exit show(items, all, short, null)
  }
  let name = argv[at]
  let words = argv[at + 1..]
  if words.is_empty() {
    exit show(items, true, short, name)
  }
  if bytes.from_text(name).len() > 15 {
    eprint f"{name}: ERROR while getting interface flags: No such device"
    exit 255
  }
  let status = configure(name, words)
  if verbose and status != 0 {
    eprint f"WARNING: at least one error occured. ({status})"
  }
  exit if status < 0 { 255 } else { status }
}
