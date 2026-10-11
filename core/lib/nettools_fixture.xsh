##! Route-netlink fixture for the net-tools applets' tests.
##!
##! Builds the interfaces, addresses, neighbours, and routes a test needs
##! inside a network namespace it owns, without calling the applets under
##! test, and runs applet command lines there. Nothing here is used by an
##! applet itself.
use nettools

pure attr(kind: Int, data: Bytes) -> Bytes {
  let length = 4 + data.len()
  let padding = (4 - length % 4) % 4
  bytes.concat([bytes.pack_le(length, 2) ?? b"", bytes.pack_le(kind, 2) ?? b"", data, bytes.zero(padding) ?? b""])
}

pure c_string(text: Str) -> Bytes {
  bytes.concat([bytes.from_text(text), b"\x00"])
}

proc send(message_type: Int, flags: Int, payload: Bytes) [process, error] {
  let c = linux.net_constants()
  let channel = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(channel)
  let replies = linux.netlink_request(channel, message_type, c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK).bit_or(flags), payload)?
  assert replies.len() >= 0
}

proc index_of(name: Str) [process, net, error] -> Int {
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  let reply = linux.ioctl(socket, c.SIOCGIFINDEX, nettools.ifreq(name, b"")?, 40)?
  bytes.unpack_le(reply, 4, 16)?
}

pure mac_bytes(text: Str) -> Bytes {
  var octets: List[Int] = []
  for part in text.split(":") {
    var value = 0
    for digit in part.lower() {
      value = value * 16 + ("0123456789abcdef".find(digit) ?? 0)
    }
    octets += [value]
  }
  bytes.from_ints(octets) ?? b""
}

## Creates a dummy interface with a fixed hardware address, still down.
export proc link_add_dummy(name: Str, mac: Str) [process, error] {
  let c = linux.net_constants()
  let kind = attr(18, attr(1, c_string("dummy")))
  let payload = bytes.concat([bytes.zero(16)?, attr(3, c_string(name)), attr(1, mac_bytes(mac)), kind])
  send(c.RTM_NEWLINK, c.NLM_F_CREATE.bit_or(c.NLM_F_EXCL), payload)
}

## Creates a veth pair, both ends still down.
export proc link_add_veth(name: Str, peer: Str, mac: Str, peer_mac: Str) [process, error] {
  let c = linux.net_constants()
  let peer_info = bytes.concat([bytes.zero(16)?, attr(3, c_string(peer)), attr(1, mac_bytes(peer_mac))])
  let data = attr(2, peer_info)
  let kind = attr(18, bytes.concat([attr(1, c_string("veth")), attr(2, data)]))
  let payload = bytes.concat([bytes.zero(16)?, attr(3, c_string(name)), attr(1, mac_bytes(mac)), kind])
  send(c.RTM_NEWLINK, c.NLM_F_CREATE.bit_or(c.NLM_F_EXCL), payload)
}

## Brings an interface up through its flags.
export proc link_up(name: Str) [process, net, error] {
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  let current = linux.ioctl(socket, c.SIOCGIFFLAGS, nettools.ifreq(name, b"")?, 40)?
  let flags = (bytes.unpack_le(current, 2, 16)?).bit_or(c.IFF_UP)
  let reply = linux.ioctl(socket, c.SIOCSIFFLAGS, nettools.ifreq(name, bytes.pack_le(flags, 2)?)?, 0)?
  assert reply.len() >= 0
}

## Assigns an IPv4 address with its broadcast address and an optional label.
export proc address_add4(name: Str, address: Str, prefix: Int, broadcast: Str, label: Str) [process, net, error] {
  let c = linux.net_constants()
  let raw = nettools.ipv4_parse(address) ?? b""
  let brd = nettools.ipv4_parse(broadcast) ?? b""
  var attributes = [attr(2, raw), attr(1, raw), attr(4, brd)]
  if label != "" { attributes += [attr(3, c_string(label))] }
  let header = bytes.concat([bytes.from_ints([c.AF_INET, prefix, 0, 0])?, bytes.pack_le(index_of(name), 4)?])
  send(c.RTM_NEWADDR, c.NLM_F_CREATE.bit_or(c.NLM_F_EXCL), bytes.concat([header, bytes.concat(attributes)]))
}

## Assigns an IPv6 address.
export proc address_add6(name: Str, address: Str, prefix: Int) [process, net, error] {
  let c = linux.net_constants()
  let raw = nettools.ipv6_parse(address) ?? b""
  let header = bytes.concat([bytes.from_ints([c.AF_INET6, prefix, 0, 0])?, bytes.pack_le(index_of(name), 4)?])
  send(c.RTM_NEWADDR, c.NLM_F_CREATE.bit_or(c.NLM_F_EXCL), bytes.concat([header, attr(2, raw), attr(1, raw)]))
}

## Adds a neighbour entry in an explicit state (`state` is the NUD_* bit).
export proc neighbour_add(name: Str, address: Str, mac: Str, state: Int) [process, net, error] {
  let c = linux.net_constants()
  let raw = nettools.ipv4_parse(address) ?? b""
  let header = bytes.concat([bytes.from_ints([c.AF_INET, 0, 0, 0])?, bytes.pack_le(index_of(name), 4)?, bytes.pack_le(state, 2)?, bytes.from_ints([0, 1])?])
  var attributes = [attr(1, raw)]
  if mac != "" { attributes += [attr(2, mac_bytes(mac))] }
  send(c.RTM_NEWNEIGH, c.NLM_F_CREATE.bit_or(c.NLM_F_EXCL), bytes.concat([header, bytes.concat(attributes)]))
}

## Adds an IPv4 route of an explicit type (RTN_*) with no nexthop.
export proc route_add_type4(destination: Str, prefix: Int, kind: Int) [process, error] {
  let c = linux.net_constants()
  let raw = nettools.ipv4_parse(destination) ?? b""
  let header = bytes.concat([bytes.from_ints([c.AF_INET, prefix, 0, 0, 254, 3, 254, kind])?, bytes.zero(4)?])
  send(c.RTM_NEWROUTE, c.NLM_F_CREATE.bit_or(c.NLM_F_EXCL), bytes.concat([header, attr(1, raw)]))
}

## Runs `script` once per command line under the namespace this process owns.
## `argv` is the xsh binary, the script, then command lines separated by `--`
## words; each line's output is printed as a transcript block that a test
## compares as text.
export proc run_commands(argv: List[Str]) [fs, process, io, error] {
  let xsh = argv[0]
  let script = argv[1]
  var current: List[Str] = []
  var blocks: List[List[Str]] = []
  for word in argv[2..] {
    if word == "--" {
      blocks += [current]
      current = []
    } else {
      current += [word]
    }
  }
  if !current.is_empty() { blocks += [current] }
  for args in blocks {
    let captured = run.capture --text $xsh $script @args
    var transcript = f"$ {args.join(" ")}\n{captured.stdout}"
    for line in captured.stderr.lines() {
      transcript += f"2> {line}\n"
    }
    transcript += f"rc={captured.status.shell_code()?}\n"
    io.write_stdout(transcript)?
  }
}
