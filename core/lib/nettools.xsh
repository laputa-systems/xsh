##! Shared model, collectors, and legacy text layout for the net-tools
##! compatibility applets ifconfig, route, and arp.
##!
##! Interfaces, addresses, IPv4 routes, and neighbours are read through route
##! netlink; interface and ARP changes go through the interface and ARP ioctls,
##! and route changes through route netlink. Only the printed layout is legacy:
##! the column widths, flag letters, and name ordering below are pinned against
##! the reference tool by the applets' tests. The IPv6 routing table is read from
##! its procfs listing because route netlink omits the reject entries the legacy
##! table shows and exposes no reference counts.

const HEX = "0123456789abcdef"
const HEX_UPPER = "0123456789ABCDEF"

## The IFF_* bits a SIOCGIFFLAGS reply carries in its low 16 bits.
export const IFF_UP = 1
## The interface has a broadcast address.
export const IFF_BROADCAST = 2
## Debugging is enabled on the interface.
export const IFF_DEBUG = 4
## The interface is a loopback device.
export const IFF_LOOPBACK = 8
## The interface is a point-to-point link.
export const IFF_POINTOPOINT = 16
## Trailer encapsulation is avoided.
export const IFF_NOTRAILERS = 32
## The interface has its resources allocated and is running.
export const IFF_RUNNING = 64
## The interface does not use ARP.
export const IFF_NOARP = 128
## The interface receives every packet.
export const IFF_PROMISC = 256
## The interface receives every multicast packet.
export const IFF_ALLMULTI = 512
## The interface is the master of a bond or team.
export const IFF_MASTER = 1024
## The interface is a slave of a bond or team.
export const IFF_SLAVE = 2048
## The interface supports multicast.
export const IFF_MULTICAST = 4096
## The interface address is dynamic.
export const IFF_DYNAMIC = 32768

const ARPHRD_ETHER = 1
const ARPHRD_TUNNEL = 768
const ARPHRD_LOOPBACK = 772

type FlagName = {bit: Int, name: Str}
type FlagLetter = {bit: Int, letter: Str}
type HardwareType = {type: Int, name: Str, description: Str, print: Str}

# The names ifconfig prints inside flags=N<...>, in its printing order.
const FLAG_NAMES = [
  {bit: 1, name: "UP"},
  {bit: 2, name: "BROADCAST"},
  {bit: 4, name: "DEBUG"},
  {bit: 8, name: "LOOPBACK"},
  {bit: 16, name: "POINTOPOINT"},
  {bit: 32, name: "NOTRAILERS"},
  {bit: 64, name: "RUNNING"},
  {bit: 128, name: "NOARP"},
  {bit: 256, name: "PROMISC"},
  {bit: 512, name: "ALLMULTI"},
  {bit: 2048, name: "SLAVE"},
  {bit: 1024, name: "MASTER"},
  {bit: 4096, name: "MULTICAST"},
  {bit: 32768, name: "DYNAMIC"},
]

# The one-letter flags of the short listing, in its printing order. Promiscuous
# and point-to-point both print as P, at different positions.
const FLAG_LETTERS = [
  {bit: 512, letter: "A"},
  {bit: 2, letter: "B"},
  {bit: 4, letter: "D"},
  {bit: 8, letter: "L"},
  {bit: 4096, letter: "M"},
  {bit: 32768, letter: "d"},
  {bit: 256, letter: "P"},
  {bit: 32, letter: "N"},
  {bit: 128, letter: "O"},
  {bit: 16, letter: "P"},
  {bit: 2048, letter: "s"},
  {bit: 1024, letter: "m"},
  {bit: 64, letter: "R"},
  {bit: 1, letter: "U"},
]

# Hardware types with a legacy name. `print` says how the address is shown:
# ether is colon-separated hex, none prints no address, empty prints an empty
# address (which keeps one more space), and anything unlisted prints as unspec.
const HARDWARE_TYPES = [
  {type: 0, name: "netrom", description: "AMPR NET/ROM", print: "none"},
  {type: 1, name: "ether", description: "Ethernet", print: "ether"},
  {type: 3, name: "ax25", description: "AMPR AX.25", print: "none"},
  {type: 6, name: "tr", description: "16/4 Mbps Token Ring", print: "none"},
  {type: 7, name: "arcnet", description: "ARCnet", print: "none"},
  {type: 256, name: "slip", description: "Serial Line IP", print: "none"},
  {type: 257, name: "cslip", description: "VJ Serial Line IP", print: "none"},
  {type: 258, name: "slip6", description: "6-bit Serial Line IP", print: "none"},
  {type: 259, name: "cslip6", description: "VJ 6-bit Serial Line IP", print: "none"},
  {type: 264, name: "adaptive", description: "Adaptive Serial Line IP", print: "none"},
  {type: 512, name: "ppp", description: "Point-to-Point Protocol", print: "none"},
  {type: 768, name: "tunnel", description: "IPIP Tunnel", print: "empty"},
  {type: 772, name: "loop", description: "Local Loopback", print: "none"},
]

## One interface's counters, named as the long listing prints them.
export type Counters = {
  rx_packets: Int, rx_bytes: Int, rx_errors: Int, rx_dropped: Int, rx_overruns: Int, rx_frame: Int,
  tx_packets: Int, tx_bytes: Int, tx_errors: Int, tx_dropped: Int, tx_overruns: Int, tx_carrier: Int,
  tx_collisions: Int,
}

## One IPv4 address with its peer, prefix length, and broadcast address.
export type Ipv4Address = {local: Str, peer: Str, prefix: Int, broadcast: Str?}
## One IPv6 address with its prefix length and route-netlink scope.
export type Ipv6Address = {address: Str, prefix: Int, scope: Int}

## One listed interface, or one IPv4 alias (a label such as eth0:1), which has
## no counters and no IPv6 addresses of its own.
export type Interface = {
  name: Str, index: Int, flags: Int, mtu: Int, hwtype: Int, hwaddr: Bytes, txqueuelen: Int,
  counters: Counters?, ipv4: Ipv4Address?, ipv6: List[Ipv6Address],
}

## One row of the IPv4 routing table with the legacy table's own fields.
export type RouteRow = {
  destination: Str, gateway: Str, mask: Str, flags: Str, metric: Int, ref: Int, use: Int, iface: Str,
  mss: Int, window: Int, irtt: Int, reject: Bool, prefix: Int,
}

## One row of the IPv6 routing table, as procfs lists it.
export type Route6Row = {destination: Str, prefix: Int, nexthop: Str, flags: Int, metric: Int, ref: Int, use: Int, iface: Str}

## One IPv4 neighbour-table entry: resolved, pending, or proxy.
export type Neighbour = {
  address: Str, hwtype: Int, hwaddr: Bytes, flags: Int, iface: Str, ifindex: Int, proxy: Bool,
}

type Attribute = {kind: Int, data: Bytes}

# ---------------------------------------------------------------- text helpers

## Drops one trailing newline.
export pure chomp(text: Str) -> Str {
  if text.ends_with("\n") { text.byte_slice(0, length: text.byte_len() - 1) } else { text }
}

pure hex_pair(value: Int, upper: Bool) -> Str {
  let digits = if upper { HEX_UPPER } else { HEX }
  f"{digits.byte_slice(value / 16, length: 1)}{digits.byte_slice(value % 16, length: 1)}"
}

## Formats the first `count` bytes of an address with a separator.
export pure address_text(raw: Bytes, count: Int, separator: Str, upper: Bool) -> Str {
  var parts: List[Str] = []
  for index in range(count) {
    parts += [hex_pair(raw.byte_at(index) ?? 0, upper)]
  }
  parts.join(separator)
}

## Dotted-quad text of four bytes starting at `offset`.
export pure ipv4_text(raw: Bytes, offset: Int) -> Str {
  [f"{raw.byte_at(offset + index) ?? 0}" for index in range(4)].join(".")
}

## Four bytes of a dotted-quad address, or null when the text is not one.
export pure ipv4_parse(text: Str) -> Bytes? {
  let parts = text.split(".")
  if parts.len() != 4 { return null }
  var octets: List[Int] = []
  for part in parts {
    if part.is_empty() or part.byte_len() > 3 { return null }
    let value = part.parse_int_decimal() ?? -1
    if value < 0 or value > 255 { return null }
    octets += [value]
  }
  match bytes.from_ints(octets) {
    Ok(raw) => raw
    Err(_) => null
  }
}

## Six bytes of an `aa:bb:cc:dd:ee:ff` hardware address (one or two hexadecimal
## digits per octet), or null for anything else, including too few or too many
## octets: the reference tool pads a short address with zeros, which would
## silently set an address the caller did not write.
export pure ether_parse(text: Str) -> Bytes? {
  let parts = text.split(":")
  if parts.len() != 6 { return null }
  var octets: List[Int] = []
  for part in parts {
    if part.byte_len() < 1 or part.byte_len() > 2 { return null }
    var value = 0
    for digit in part.lower() {
      let at = HEX.find(digit)
      if at == null { return null }
      value = value * 16 + (at ?? 0)
    }
    octets += [value]
  }
  match bytes.from_ints(octets) {
    Ok(raw) => raw
    Err(_) => null
  }
}

## A netmask given as dotted quad or as 0x-prefixed hexadecimal.
export pure netmask_parse(text: Str) -> Bytes? {
  if text.starts_with("0x") or text.starts_with("0X") {
    let value = text.parse_int() ?? -1
    if value < 0 or value > 4294967295 { return null }
    return match bytes.pack_be(value, 4) {
      Ok(raw) => raw
      Err(_) => null
    }
  }
  ipv4_parse(text)
}

# The octet value that carries 0 to 8 leading one bits.
const OCTET_BY_BITS = [0, 128, 192, 224, 240, 248, 252, 254, 255]

## The netmask bytes for a prefix length of 0 to 32.
export pure prefix_mask(prefix: Int) -> Bytes {
  var octets: List[Int] = []
  for index in range(4) {
    let bits = prefix - index * 8
    let clamped = if bits > 8 { 8 } else if bits < 0 { 0 } else { bits }
    octets += [OCTET_BY_BITS[clamped]]
  }
  match bytes.from_ints(octets) {
    Ok(raw) => raw
    Err(_) => b""
  }
}

## The prefix length of a contiguous netmask, or null for a mask with holes.
export pure mask_prefix(mask: Bytes) -> Int? {
  for prefix in range(33) {
    if prefix_mask(prefix) == mask { return prefix }
  }
  null
}

## Sixteen bytes of an IPv6 address in text form, or null when it is not one.
export pure ipv6_parse(text: Str) -> Bytes? {
  if text.is_empty() or text.find(":") == null { return null }
  var body = text
  var tail: List[Int] = []
  # An embedded IPv4 tail takes the place of the last two groups.
  let last = body.split(":")
  let final = last[last.len() - 1]
  if final.find(".") != null {
    let quad = ipv4_parse(final)
    guard let raw = quad else { return null }
    tail = [(raw.byte_at(0) ?? 0) * 256 + (raw.byte_at(1) ?? 0), (raw.byte_at(2) ?? 0) * 256 + (raw.byte_at(3) ?? 0)]
    body = body.byte_slice(0, length: body.byte_len() - final.byte_len()) + "0:0"
  }
  let halves = body.split("::")
  if halves.len() > 2 { return null }
  var groups: List[Int] = []
  var lead: List[Int] = []
  var rest: List[Int] = []
  for index, half in halves {
    var parsed: List[Int] = []
    if !half.is_empty() {
      for piece in half.split(":") {
        if piece.is_empty() or piece.byte_len() > 4 { return null }
        var value = 0
        for digit in piece.lower() {
          let at = HEX.find(digit)
          if at == null { return null }
          value = value * 16 + at
        }
        parsed += [value]
      }
    }
    if index == 0 { lead = parsed } else { rest = parsed }
  }
  if halves.len() == 1 {
    groups = lead
    if groups.len() != 8 { return null }
  } else {
    let missing = 8 - lead.len() - rest.len()
    if missing < 1 { return null }
    groups = lead + [0 for _ in range(missing)] + rest
  }
  if tail.len() == 2 {
    groups = groups[0..6] + tail
  }
  var octets: List[Int] = []
  for value in groups {
    octets += [value / 256, value % 256]
  }
  match bytes.from_ints(octets) {
    Ok(raw) => raw
    Err(_) => null
  }
}

## Parses a decimal number, returning null for anything else.
export pure decimal(text: Str) -> Int? {
  if text.is_empty() { return null }
  for digit in text {
    if HEX.find(digit) == null or (HEX.find(digit) ?? 0) > 9 { return null }
  }
  match text.parse_int_decimal() {
    Ok(value) => value
    Err(_) => null
  }
}

# ------------------------------------------------------------ interface model

## The flags field as the legacy tool prints it: a signed 16-bit number.
export pure signed16(flags: Int) -> Int {
  if flags >= 32768 { flags - 65536 } else { flags }
}

## A byte count with the 1024-step unit the long listing appends, truncated
## (not rounded) to one decimal.
export pure human_bytes(count: Int) -> Str {
  var divisor = 1
  var unit = "B"
  if count > 1099511627776 {
    divisor = 1099511627776
    unit = "TiB"
  } else if count > 1073741824 {
    divisor = 1073741824
    unit = "GiB"
  } else if count > 1048576 {
    divisor = 1048576
    unit = "MiB"
  } else if count > 1024 {
    divisor = 1024
    unit = "KiB"
  }
  # The reference truncates to one decimal rather than rounding.
  let tenths = count * 10 / divisor
  f"{tenths / 10}.{tenths % 10} {unit}"
}

pure all_digits(raw: Bytes, from: Int) -> Bool {
  if from >= raw.len() { return false }
  for index in range(from, raw.len()) {
    let byte = raw.byte_at(index) ?? 0
    if byte < 48 or byte > 57 { return false }
  }
  true
}

pure number_from(raw: Bytes, from: Int) -> Int {
  var value = 0
  for index in range(from, raw.len()) {
    value = value * 10 + (raw.byte_at(index) ?? 48) - 48
  }
  value
}

## The legacy interface-name order: byte order, except that where the names
## first differ and both remainders are plain numbers those numbers compare
## numerically, so eth2 sorts before eth10 while eth10 sorts before eth2a.
## Returns a negative number, zero, or a positive number.
export pure name_order(left: Str, right: Str) -> Int {
  let a = bytes.from_text(left)
  let b = bytes.from_text(right)
  var at = 0
  while at < a.len() and at < b.len() and a.byte_at(at) == b.byte_at(at) {
    at += 1
  }
  if at == a.len() and at == b.len() { return 0 }
  if all_digits(a, at) and all_digits(b, at) {
    let x = number_from(a, at)
    let y = number_from(b, at)
    if x != y { return if x < y { -1 } else { 1 } }
  }
  let ca = if at < a.len() { a.byte_at(at) ?? 0 } else { -1 }
  let cb = if at < b.len() { b.byte_at(at) ?? 0 } else { -1 }
  if ca < cb { -1 } else { 1 }
}

## Returns the interfaces in the legacy listing order.
export pure sort_interfaces(items: List[Interface]) -> List[Interface] {
  var sorted: List[Interface] = []
  for item in items {
    var at = sorted.len()
    for index in range(sorted.len()) {
      if name_order(sorted[index].name, item.name) > 0 {
        at = index
        break
      }
    }
    sorted = sorted[0..at] + [item] + sorted[at..]
  }
  sorted
}

pure attribute_data(attributes: List[Attribute], kind: Int) -> Bytes? {
  for attribute in attributes {
    if attribute.kind.bit_and(16383) == kind { return attribute.data }
  }
  null
}

## Splits netlink attributes that follow a fixed header of `start` bytes.
export pure attributes_of(payload: Bytes, start: Int) -> List[Attribute] {
  var found: List[Attribute] = []
  var offset = start
  while offset + 4 <= payload.len() {
    let length = bytes.unpack_le(payload, 2, offset) ?? 0
    let kind = bytes.unpack_le(payload, 2, offset + 2) ?? 0
    if length < 4 or offset + length > payload.len() { break }
    found += [{kind: kind.bit_and(16383), data: payload.slice(offset + 4, length - 4)}]
    offset += (length + 3) / 4 * 4
  }
  found
}

pure counter_field(data: Bytes, width: Int, index: Int) -> Int {
  bytes.unpack_le(data, width, index * width) ?? 0
}

# IFLA_STATS64 (23) is rtnl_link_stats64; the 32-bit IFLA_STATS (7) has the
# same field order. The long listing's grouped columns add the same members
# that the legacy /proc/net/dev reader folded together.
pure counters_of(attributes: List[Attribute]) -> Counters? {
  var width = 8
  var raw = attribute_data(attributes, 23)
  if raw == null {
    width = 4
    raw = attribute_data(attributes, 7)
  }
  guard let data = raw else { return null }
  {
    rx_packets: counter_field(data, width, 0),
    rx_bytes: counter_field(data, width, 2),
    rx_errors: counter_field(data, width, 4),
    rx_dropped: counter_field(data, width, 6) + counter_field(data, width, 15),
    rx_overruns: counter_field(data, width, 14),
    rx_frame: counter_field(data, width, 10) + counter_field(data, width, 11) + counter_field(data, width, 12) + counter_field(data, width, 13),
    tx_packets: counter_field(data, width, 1),
    tx_bytes: counter_field(data, width, 3),
    tx_errors: counter_field(data, width, 5),
    tx_dropped: counter_field(data, width, 7),
    tx_overruns: counter_field(data, width, 18),
    tx_carrier: counter_field(data, width, 17) + counter_field(data, width, 16) + counter_field(data, width, 20) + counter_field(data, width, 19),
    tx_collisions: counter_field(data, width, 9),
  }
}

## Reads every link and its addresses in one snapshot: one entry per link plus
## one per IPv4 label that differs from its link's name, in listing order.
export proc interfaces() [process, error] -> Result[List[Interface], Error] {
  let dump = linux.network_dump()?
  if dump.state == "failed" or dump.links.is_empty() {
    return Err(error.failure("could not read the interface list from the kernel"))
  }
  var found: List[Interface] = []
  for link in dump.links {
    guard let name = link.name else { continue }
    let hwaddr = link.address ?? b""
    var txqueuelen = 0
    if let raw = attribute_data(link.attributes, 13) {
      txqueuelen = bytes.unpack_le(raw, 4) ?? 0
    }
    var primary: Ipv4Address? = null
    var v6: List[Ipv6Address] = []
    var labels: List[Interface] = []
    let base = {
      name: name, index: link.ifindex, flags: link.flags.bit_and(65535), mtu: link.mtu ?? 0, hwtype: link.hardware_type,
      hwaddr: hwaddr, txqueuelen: txqueuelen, counters: counters_of(link.attributes), ipv4: null, ipv6: [],
    }
    for address in dump.addresses {
      continue when address.ifindex != link.ifindex
      if address.family == "inet6" {
        v6 += [{address: address.address ?? "", prefix: address.prefix_length, scope: address.scope}]
      } else if address.family == "inet" {
        let local = address.local ?? address.address ?? ""
        let entry = {local: local, peer: address.address ?? local, prefix: address.prefix_length, broadcast: address.broadcast}
        let label = address.label ?? name
        if label == name {
          if primary == null { primary = entry }
        } else {
          labels += [{...base, name: label, counters: null, ipv4: entry}]
        }
      }
    }
    found += [{...base, ipv4: primary, ipv6: v6}]
    for alias in labels {
      var seen = false
      for existing in found {
        if existing.name == alias.name { seen = true }
      }
      if !seen { found += [alias] }
    }
  }
  Ok(sort_interfaces(found))
}

# ------------------------------------------------------------ long and short

pure flag_names(flags: Int) -> Str {
  var names: List[Str] = []
  for entry in FLAG_NAMES {
    if flags.bit_and(entry.bit) != 0 { names += [entry.name] }
  }
  names.join(",")
}

## The letters of the short listing's Flg column.
export pure flag_letters(flags: Int) -> Str {
  var letters = ""
  for entry in FLAG_LETTERS {
    if flags.bit_and(entry.bit) != 0 { letters += entry.letter }
  }
  if letters.is_empty() { "[NO FLAGS]" } else { letters }
}

pure hardware_of(hwtype: Int) -> HardwareType? {
  for entry in HARDWARE_TYPES {
    if entry.type == hwtype { return entry }
  }
  null
}

## The legacy name of a hardware type, or `unspec`.
export pure hardware_name(hwtype: Int) -> Str {
  if let entry = hardware_of(hwtype) { entry.name } else { "unspec" }
}

pure scope_text(scope: Int) -> Str {
  if scope == 0 { return "0x0<global>" }
  if scope == 254 { return "0x10<host>" }
  if scope == 253 { return "0x20<link>" }
  if scope == 200 { return "0x40<site>" }
  "0x0<global>"
}

pure scope_id(scope: Int) -> Int {
  if scope == 254 { 16 } else if scope == 253 { 32 } else if scope == 200 { 64 } else { 0 }
}

pure scope_label(scope: Int) -> Str {
  if scope == 254 { "host" } else if scope == 253 { "link" } else if scope == 200 { "site" } else { "global" }
}

pure hardware_line(item: Interface) -> Str {
  let known = hardware_of(item.hwtype)
  if let entry = known {
    var blank = true
    for byte in item.hwaddr {
      if byte != 0 { blank = false }
    }
    var shown = ""
    if entry.print == "ether" {
      shown = f"{address_text(item.hwaddr, 6, ":", false)} "
    } else if entry.print == "empty" {
      shown = " "
    }
    if entry.type == ARPHRD_LOOPBACK and blank { shown = "" }
    return f"        {entry.name} {shown} txqueuelen {item.txqueuelen}  ({entry.description})"
  }
  var padded = item.hwaddr.slice(0, if item.hwaddr.len() < 14 { item.hwaddr.len() } else { 14 })
  let missing = 16 - padded.len()
  padded = bytes.concat([padded, bytes.zero(missing) ?? b""])
  f"        unspec {address_text(padded, 16, "-", true)}  txqueuelen {item.txqueuelen}  (UNSPEC)"
}

## The multi-line listing of one interface, ending with a blank line.
export pure long_listing(item: Interface) -> Str {
  var lines: List[Str] = []
  let flags = signed16(item.flags)
  lines += [f"{item.name}: flags={flags}<{flag_names(item.flags)}>  mtu {item.mtu}"]
  if let address = item.ipv4 {
    let mask = ipv4_text(prefix_mask(address.prefix), 0)
    var text = f"        inet {address.local}  netmask {mask}"
    if item.flags.bit_and(IFF_POINTOPOINT) != 0 { text += f"  destination {address.peer}" }
    if item.flags.bit_and(IFF_BROADCAST) != 0 { text += f"  broadcast {address.broadcast ?? "0.0.0.0"}" }
    lines += [text]
  }
  for address in item.ipv6 {
    lines += [f"        inet6 {address.address}  prefixlen {address.prefix}  scopeid {scope_text(address.scope)}"]
  }
  lines += [hardware_line(item)]
  if let c = item.counters {
    lines += [
      f"        RX packets {c.rx_packets}  bytes {c.rx_bytes} ({human_bytes(c.rx_bytes)})",
      f"        RX errors {c.rx_errors}  dropped {c.rx_dropped}  overruns {c.rx_overruns}  frame {c.rx_frame}",
      f"        TX packets {c.tx_packets}  bytes {c.tx_bytes} ({human_bytes(c.tx_bytes)})",
      f"        TX errors {c.tx_errors}  dropped {c.tx_dropped} overruns {c.tx_overruns}  carrier {c.tx_carrier}  collisions {c.tx_collisions}",
    ]
  }
  lines.join("\n") + "\n\n"
}

## The header of the short (-s) listing.
export const SHORT_HEADER = "Iface      MTU    RX-OK RX-ERR RX-DRP RX-OVR    TX-OK TX-ERR TX-DRP TX-OVR Flg"

## One row of the short listing. An alias has no statistics to show.
export pure short_row(item: Interface) -> Str {
  let head = f"{item.name:<15} {item.mtu:>5}"
  let letters = flag_letters(item.flags)
  if let c = item.counters {
    return f"{head} {c.rx_packets:>8} {c.rx_errors:>6} {c.rx_dropped:>6} {c.rx_overruns:<6} {c.tx_packets:>8} {c.tx_errors:>6} {c.tx_dropped:>6} {c.tx_overruns:>6} {letters}"
  }
  f"{head}      - no statistics available -                        {letters}"
}

# --------------------------------------------------------------------- routes

## The IPv4 routing table the way the legacy tool lists it: the main table's
## unicast, reject, blackhole, and throw routes in kernel order.
export proc ipv4_routes() [process, error] -> Result[List[RouteRow], Error] {
  let dump = linux.network_dump()?
  var names: Map[Int, Str] = {}
  for link in dump.links {
    names = names.set(link.ifindex, link.name ?? "")
  }
  var rows: List[RouteRow] = []
  for route in dump.routes {
    continue when route.family != "inet" or route.table != 254
    # RTN_BROADCAST and RTN_MULTICAST never appear in the legacy table.
    continue when route.route_type == 3 or route.route_type == 5
    var gateway = route.gateway ?? "0.0.0.0"
    var ifindex = route.output_ifindex ?? 0
    if route.nexthops.len() > 0 and ifindex == 0 {
      ifindex = route.nexthops[0].ifindex
      gateway = route.nexthops[0].gateway ?? gateway
    }
    let reject = route.route_type == 7 or route.route_type == 8
    var mss = 0
    var window = 0
    var irtt = 0
    if let metrics = attribute_data(route.attributes, 8) {
      for metric in attributes_of(metrics, 0) {
        let value = bytes.unpack_le(metric.data, 4) ?? 0
        if metric.kind == 8 { mss = value + 40 }
        if metric.kind == 3 { window = value }
        if metric.kind == 4 { irtt = value / 8 }
      }
    }
    var flags = ""
    if reject {
      flags = "!"
    } else {
      flags = "U"
      if gateway != "0.0.0.0" { flags += "G" }
      if route.destination_prefix_length == 32 { flags += "H" }
    }
    rows += [{
      destination: route.destination ?? "0.0.0.0",
      gateway: gateway,
      mask: ipv4_text(prefix_mask(route.destination_prefix_length), 0),
      flags: flags,
      metric: route.priority ?? 0,
      ref: 0,
      use: 0,
      iface: names.get(ifindex) ?? "*",
      mss: mss,
      window: window,
      irtt: irtt,
      reject: reject,
      prefix: route.destination_prefix_length,
    }]
  }
  Ok(rows)
}

pure hex_int(text: Str) -> Int {
  var value = 0
  for digit in text.lower() {
    value = value * 16 + (HEX.find(digit) ?? 0)
  }
  value
}

## Reads the IPv6 routing table from its procfs listing.
export proc ipv6_routes() [fs, error] -> Result[List[Route6Row], Error] {
  var rows: List[Route6Row] = []
  for line in fp"/proc/net/ipv6_route".read_text()?.lines() {
    let f = line.fields()
    continue when f.len() < 10
    let metric = hex_int(f[5])
    rows += [{
      destination: ipv6_from_hex(f[0]),
      prefix: hex_int(f[1]),
      nexthop: ipv6_from_hex(f[4]),
      flags: hex_int(f[8]),
      metric: if metric >= 2147483648 { metric - 4294967296 } else { metric },
      ref: hex_int(f[6]),
      use: hex_int(f[7]),
      iface: f[9],
    }]
  }
  Ok(rows)
}

pure ipv6_from_hex(text: Str) -> Str {
  var groups: List[Str] = []
  for index in range(8) {
    groups += [text.byte_slice(index * 4, length: 4)]
  }
  compress_ipv6(groups.join(":"))
}

## The canonical text of an IPv6 address given as eight hexadecimal groups.
export pure compress_ipv6(text: Str) -> Str {
  var values: List[Int] = []
  for piece in text.split(":") {
    values += [hex_int(piece)]
  }
  var best_at = -1
  var best_len = 0
  var at = 0
  while at < 8 {
    if values[at] == 0 {
      var end = at
      while end < 8 and values[end] == 0 { end += 1 }
      if end - at > best_len {
        best_len = end - at
        best_at = at
      }
      at = end
    } else {
      at += 1
    }
  }
  if best_len < 2 { best_at = -1 }
  var out = ""
  var index = 0
  while index < 8 {
    if index == best_at {
      out += "::"
      index += best_len
      continue
    }
    if !out.is_empty() and !out.ends_with(":") { out += ":" }
    out += hex_group(values[index])
    index += 1
  }
  out
}

pure hex_group(value: Int) -> Str {
  if value == 0 { return "0" }
  var text = ""
  var rest = value
  while rest > 0 {
    text = HEX.byte_slice(rest % 16, length: 1) + text
    rest = rest / 16
  }
  text
}

pure ipv6_flag_letters(flags: Int) -> Str {
  var letters = ""
  if flags.bit_and(1) != 0 { letters += "U" }
  if flags.bit_and(512) != 0 { letters += "!" }
  if flags.bit_and(2) != 0 { letters += "G" }
  if flags.bit_and(4) != 0 { letters += "H" }
  if flags.bit_and(8) != 0 { letters += "R" }
  if flags.bit_and(16) != 0 { letters += "D" }
  if flags.bit_and(32) != 0 { letters += "M" }
  if flags.bit_and(262144) != 0 { letters += "A" }
  if flags.bit_and(16777216) != 0 { letters += "C" }
  if flags.bit_and(2097152) != 0 { letters += "n" }
  letters
}

## The IPv6 routing table header.
export const ROUTE6_HEADER = "Kernel IPv6 routing table\nDestination                    Next Hop                   Flag Met Ref  Use If"

## Route6 row.
export pure route6_row(row: Route6Row, destination: Str, nexthop: Str) -> Str {
  f"{destination:<30} {nexthop:<26} {ipv6_flag_letters(row.flags):<4} {row.metric:<3} {row.ref:<3} {row.use:>4} {row.iface}"
}

## The header of the IPv4 table.
export const ROUTE4_HEADER = "Kernel IP routing table\nDestination     Gateway         Genmask         Flags Metric Ref    Use Iface"
## Route4 extended header.
## The header of the IPv4 table under -e.
export const ROUTE4_EXTENDED_HEADER = "Kernel IP routing table\nDestination     Gateway         Genmask         Flags   MSS Window  irtt Iface"
## Route4 extra header.
## The header of the IPv4 table under -ee.
export const ROUTE4_EXTRA_HEADER = "Kernel IP routing table\nDestination     Gateway         Genmask         Flags Metric Ref    Use Iface    MSS   Window irtt"
## Route4 cache header.
## The header of the IPv4 routing cache listing.
export const ROUTE4_CACHE_HEADER = "Kernel IP routing cache\nSource          Destination     Gateway         Flags Metric Ref    Use Iface"

## One row of the IPv4 table. `extend` is the number of -e flags given.
export pure route4_row(row: RouteRow, destination: Str, gateway: Str, extend: Int) -> Str {
  let dash = row.reject
  let mss = if dash { "-" } else { f"{row.mss}" }
  let window = if dash { "-" } else { f"{row.window}" }
  let irtt = if dash { "-" } else { f"{row.irtt}" }
  let iface = if dash { "-" } else { row.iface }
  let gate = if dash { "-" } else { gateway }
  let ref = if dash { "-" } else { f"{row.ref}" }
  if extend == 1 {
    return f"{destination:<15} {gate:<15} {row.mask:<15} {row.flags:<5} {mss:>5} {window:<6} {irtt:>5} {iface}"
  }
  let base = f"{destination:<15} {gate:<15} {row.mask:<15} {row.flags:<5} {row.metric:<6} {ref:<2} {row.use:>7}"
  if extend >= 2 {
    return f"{base} {iface:<8} {mss:<5} {window:<6} {irtt}"
  }
  f"{base} {iface}"
}

# ----------------------------------------------------------------- neighbours

## Reads the IPv4 neighbour table: resolved and pending entries and the
## proxy entries, without the NOARP entries the legacy listing skips.
export proc neighbours() [process, error] -> Result[List[Neighbour], Error] {
  let c = linux.net_constants()
  let dump = linux.network_dump()?
  var link_names: Map[Int, Str] = {}
  var link_types: Map[Int, Int] = {}
  for link in dump.links {
    link_names = link_names.set(link.ifindex, link.name ?? "")
    link_types = link_types.set(link.ifindex, link.hardware_type)
  }
  let fd = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(fd)
  var found: List[Neighbour] = []
  # Proxy entries sit in a separate table that a dump only walks when the
  # request carries NTF_PROXY, so the neighbour table is asked for twice.
  for proxy_table in [false, true] {
    let marker = if proxy_table { 8 } else { 0 }
    let request = bytes.from_ints([c.AF_INET, 0, 0, 0, 0, 0, 0, 0, 0, 0, marker, 0])?
    let replies = linux.netlink_request(fd, c.RTM_GETNEIGH, c.NLM_F_REQUEST.bit_or(c.NLM_F_DUMP), request)?
    for reply in replies {
      continue when reply.type != c.RTM_NEWNEIGH or reply.payload.len() < 12
      let ifindex = bytes.unpack_le(reply.payload, 4, 4)?
      let state = bytes.unpack_le(reply.payload, 2, 8)?
      let ndm_flags = reply.payload.byte_at(10) ?? 0
      continue when state.bit_and(64) != 0
      var address = ""
      var hwaddr = b""
      for attribute in attributes_of(reply.payload, 12) {
        if attribute.kind == 1 and attribute.data.len() == 4 { address = ipv4_text(attribute.data, 0) }
        if attribute.kind == 2 { hwaddr = attribute.data }
      }
      continue when address == ""
      let proxy = ndm_flags.bit_and(8) != 0
      continue when proxy != proxy_table
      var flags = 0
      if proxy {
        flags = 12
      } else if state.bit_and(128) != 0 {
        flags = 6
      } else if state.bit_and(222) != 0 {
        flags = 2
      }
      found += [{
        address: address,
        hwtype: link_types.get(ifindex) ?? 1,
        hwaddr: hwaddr,
        flags: flags,
        iface: link_names.get(ifindex) ?? "*",
        ifindex: ifindex,
        proxy: proxy,
      }]
    }
  }
  Ok(found)
}

## The default-style row of the ARP listing. `name` replaces the address when
## it was resolved.
export pure arp_row(entry: Neighbour, name: Str) -> Str {
  let complete = entry.flags.bit_and(2) != 0
  var kind = ""
  var hw = "(incomplete)"
  if complete {
    kind = hardware_name(entry.hwtype)
    hw = address_text(entry.hwaddr, if entry.hwaddr.len() < 6 { entry.hwaddr.len() } else { 6 }, ":", false)
  } else if entry.flags.bit_and(8) != 0 {
    kind = "*"
    hw = "<from_interface>"
  }
  var letters = ""
  if entry.flags.bit_and(2) != 0 { letters += "C" }
  if entry.flags.bit_and(4) != 0 { letters += "M" }
  if entry.flags.bit_and(8) != 0 { letters += "P" }
  f"{name:<24} {kind:<7} {hw:<19} {letters:<5} {"":<15} {entry.iface}"
}

## The header of the ARP listing.
export const ARP_HEADER = "Address                  HWtype  HWaddress           Flags Mask            Iface"

## The BSD-style (-a) row of the ARP listing.
export pure arp_bsd_row(entry: Neighbour, name: Str) -> Str {
  let complete = entry.flags.bit_and(2) != 0
  var hw = "<incomplete>"
  if complete {
    hw = address_text(entry.hwaddr, if entry.hwaddr.len() < 6 { entry.hwaddr.len() } else { 6 }, ":", false)
  } else if entry.flags.bit_and(8) != 0 {
    hw = "<from_interface>"
  }
  var text = f"{name} ({entry.address}) at {hw}"
  if complete { text += f" [{hardware_name(entry.hwtype)}]" }
  if entry.flags.bit_and(4) != 0 { text += " PERM" }
  if entry.flags.bit_and(8) != 0 { text += " PUB" }
  f"{text} on {entry.iface}"
}

# --------------------------------------------------------- names and requests

## Reverse-resolves addresses through the hosts file only; a numeric listing
## and an address with no hosts entry both fall back to the caller's default.
export proc hosts_names() [fs] -> Map[Str, Str] {
  var names: Map[Str, Str] = {}
  let text = fp"/etc/hosts".read_text() ?? ""
  for line in text.lines() {
    let content = line.split("#")[0]
    let words = content.words()
    continue when words.len() < 2
    if words[0] not in names.keys() { names = names.set(words[0], words[1]) }
  }
  names
}

## Resolves a host name through the hosts file, forward only.
export proc hosts_address(name: Str) [fs] -> Str? {
  let text = fp"/etc/hosts".read_text() ?? ""
  for line in text.lines() {
    let words = line.split("#")[0].words()
    continue when words.len() < 2
    if name in words[1..] and ipv4_parse(words[0]) != null { return words[0] }
  }
  null
}

## Builds a 40-byte `struct ifreq`: the zero-padded name, then `tail`.
export pure ifreq(name: Str, tail: Bytes) -> Result[Bytes, Error] {
  let raw = bytes.from_text(name)
  if raw.len() > 15 or tail.len() > 24 { return Err(error.failure(f"{name}: interface name too long")) }
  Ok(bytes.concat([raw, bytes.zero(16 - raw.len())?, tail, bytes.zero(24 - tail.len())?]))
}
