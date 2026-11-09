##! Typed links, IPv4/IPv6 addresses and routes of the caller's network
##! namespace, observed from one route-netlink dump and changed one request at
##! a time. Commands render these records and controllers plan over them; no
##! caller reparses another's text. An observation that the kernel interrupted,
##! truncated or answered with undecodable objects is an error naming why, never
##! an empty or partial state, and a notification stream that lost messages
##! says so, so the caller re-observes instead of trusting a stale view.
use nettools

## An address family this library observes and changes.
export enum Family: Str { Inet = "inet", Inet6 = "inet6" }

## One link. `flags` holds the kernel's IFF_* bits.
export type Link = {
  ifindex: Int,
  name: Str,
  kind: Str?,
  hardware_type: Int,
  mac: Bytes?,
  mtu: Int?,
  flags: Int,
  master_ifindex: Int?,
  lower_ifindex: Int?,
}

## One interface address. `peer` is the far end of a point-to-point address
## and null otherwise; `flags` holds the kernel's IFA_F_* bits.
export type Address = {
  ifindex: Int,
  family: Family,
  local: Str,
  prefix: Int,
  peer: Str?,
  broadcast: Str?,
  scope: Int,
  flags: Int,
}

## One next hop of a route. A blackhole or unreachable route has one hop
## with neither a link nor a gateway.
export type Hop = {ifindex: Int?, gateway: Str?}

## One route. `other_attributes` lists the route-netlink attribute kinds the
## kernel reported that this record does not model, such as metrics or an
## IPv6 cache entry, so a caller can tell an ordinary route from a customized
## one without reading raw attributes.
export type Route = {
  family: Family,
  table: Int,
  destination: Str?,
  prefix: Int,
  source: Str?,
  source_prefix: Int,
  preferred_source: Str?,
  tos: Int,
  priority: Int,
  protocol: Int,
  scope: Int,
  route_type: Int,
  flags: Int,
  hops: List[Hop],
  other_attributes: List[Int],
}

## Objects of a family this library does not model, counted per dump. They
## are left out of `State` and reported here rather than dropped silently.
export type Unmodeled = {object: Str, family: Str, count: Int}

## One complete observation of links, addresses and routes.
export type State = {links: List[Link], addresses: List[Address], routes: List[Route], unmodeled: List[Unmodeled]}

## Why no trustworthy observation exists. Each variant names the dump that
## failed; every one of them means "observe again or give up", never "empty".
export error ObserveFailure =
  Unsupported(object: Str, message: Str) |
  Denied(object: Str, message: Str) |
  Malformed(object: Str, message: Str) |
  Truncated(object: Str, message: Str) |
  Interrupted(object: Str, message: Str) |
  Limited(object: Str, message: Str) |
  Failed(object: Str, errno: Int?, message: Str)

## One change to links, addresses or routes.
export enum Change {
  LinkUp(Int),
  LinkDown(Int),
  AddAddress(Address),
  RemoveAddress(Address),
  AddRoute(Route),
  RemoveRoute(Route),
}

## One encoded route-netlink request.
export type Request = {message_type: Int, flags: Int, payload: Bytes}

## Why a change was not made. `Refused` carries the kernel's errno; the
## others mean the request never reached a kernel verdict.
export error ChangeFailure =
  Refused(errno: Int, message: Str) |
  Unencodable(message: Str) |
  Transport(message: Str)

## What a drained notification socket says about the last observation.
export enum Notices {
  Quiet,
  Changed,
  Lost(Str),
}

## The main routing table.
export const TABLE_MAIN = 254
## The longest a change waits for the kernel's acknowledgement, in milliseconds.
export const CHANGE_TIMEOUT_MS = 3000

const OBSERVED_OBJECTS = ["socket", "link", "address", "route"]
const MODELED_ROUTE_ATTRIBUTES = [1, 2, 4, 5, 6, 7, 9, 15]
const RTA_SRC = 2
const RTM_F_CLONED = 512
const ENOBUFS = 105
const EAGAIN_CODES = [11, 35]
const MAX_NOTICE_READS = 64
const NOTICE_BYTES = 65536
const NLMSG_HEADER = 16

const LINK_FLAG_NAMES = [
  {bit: 1, name: "UP"},
  {bit: 2, name: "BROADCAST"},
  {bit: 8, name: "LOOPBACK"},
  {bit: 64, name: "RUNNING"},
  {bit: 4096, name: "MULTICAST"},
  {bit: 256, name: "PROMISC"},
  {bit: 128, name: "NOARP"},
  {bit: 16, name: "POINTOPOINT"},
]

const ERRNO_TEXT = [
  {errno: 1, text: "Operation not permitted"},
  {errno: 2, text: "No such file or directory"},
  {errno: 3, text: "No such process"},
  {errno: 16, text: "Device or resource busy"},
  {errno: 17, text: "File exists"},
  {errno: 19, text: "No such device"},
  {errno: 22, text: "Invalid argument"},
  {errno: 34, text: "Numerical result out of range"},
  {errno: 95, text: "Operation not supported"},
  {errno: 97, text: "Address family not supported by protocol"},
  {errno: 98, text: "Address already in use"},
  {errno: 99, text: "Cannot assign requested address"},
  {errno: 100, text: "Network is down"},
  {errno: 101, text: "Network is unreachable"},
  {errno: 105, text: "No buffer space available"},
  {errno: 113, text: "No route to host"},
]

## The `strerror` text of an errno a route-netlink request can return, or
## `errno N` for one outside that set.
export pure errno_text(errno: Int) -> Str {
  for entry in ERRNO_TEXT {
    return entry.text when entry.errno == errno
  }
  f"errno {errno}"
}

## The names of the IFF_* bits `ip` and `ifconfig` show, in their order.
export pure link_flag_names(flags: Int) -> List[Str] {
  [entry.name for entry in LINK_FLAG_NAMES if flags.bit_and(entry.bit) != 0]
}

## True when the link is administratively up.
export pure is_up(link: Link) -> Bool {
  link.flags.bit_and(linux.net_constants().IFF_UP) != 0
}

## True when the link reports carrier.
export pure has_carrier(link: Link) -> Bool {
  link.flags.bit_and(linux.net_constants().IFF_LOWER_UP) != 0
}

## True for a route the kernel cloned into its cache rather than one a
## caller installed.
export pure is_cloned(route: Route) -> Bool {
  route.flags.bit_and(RTM_F_CLONED) != 0
}

## The link with `ifindex`, or null.
export pure link_by_index(state: State, ifindex: Int) -> Link? {
  for link in state.links {
    return link when link.ifindex == ifindex
  }
  null
}

## The addresses on the link with `ifindex`, in kernel order.
export pure addresses_of(state: State, ifindex: Int) -> List[Address] {
  [address for address in state.addresses if address.ifindex == ifindex]
}

## The unspecified address of a family, as the kernel prints it.
export pure unspecified(family: Family) -> Str {
  if family == .Inet { "0.0.0.0" } else { "::" }
}

pure issue_failure(issue: LinuxNetworkIssue) -> ObserveFailure {
  let object = issue.object
  let message = issue.message
  match issue.state {
    "unsupported" => .Unsupported(object:, message:)
    "permission_denied" => .Denied(object:, message:)
    "malformed" => .Malformed(object:, message:)
    "truncated" => .Truncated(object:, message:)
    "interrupted" => .Interrupted(object:, message:)
    "limited" => .Limited(object:, message:)
    else => .Failed(object:, errno: issue.errno, message:)
  }
}

pure family_of(text: Str) -> Family? {
  match text {
    "inet" => .Inet
    "inet6" => .Inet6
    else => null
  }
}

pure count_unmodeled(object: Str, families: List[Str]) -> List[Unmodeled] {
  var seen: List[Str] = []
  for family in families {
    seen += [family] unless family in seen
  }
  [{object, family, count: [item for item in families if item == family].len()} for family in seen]
}

pure link_record(entry: LinuxNetworkLink) -> Result[Link, ObserveFailure] {
  guard let name = entry.name else { fail .Malformed(object: "link", message: f"link {entry.ifindex} has no UTF-8 name") }
  Ok({
    ifindex: entry.ifindex,
    name,
    kind: entry.kind,
    hardware_type: entry.hardware_type,
    mac: entry.address,
    mtu: entry.mtu,
    flags: entry.flags,
    master_ifindex: entry.master_ifindex,
    lower_ifindex: entry.lower_ifindex,
  })
}

pure address_record(entry: LinuxNetworkAddress, family: Family) -> Result[Address, ObserveFailure] {
  let primary: Str? = if entry.local != null { entry.local } else { entry.address }
  guard let local = primary else { fail .Malformed(object: "address", message: f"address on link {entry.ifindex} has neither a local nor a peer address") }
  let peer = if entry.local != null and entry.address != null and entry.address != entry.local { entry.address } else { null }
  Ok({ifindex: entry.ifindex, family, local, prefix: entry.prefix_length, peer, broadcast: entry.broadcast, scope: entry.scope, flags: entry.flags})
}

pure route_record(entry: LinuxNetworkRoute, family: Family) -> Route {
  let hops = if entry.nexthops.is_empty() {
    [{ifindex: entry.output_ifindex, gateway: entry.gateway}]
  } else {
    [{ifindex: hop.ifindex, gateway: hop.gateway} for hop in entry.nexthops]
  }
  var other: List[Int] = []
  for attribute in entry.attributes {
    other += [attribute.kind] unless attribute.kind in MODELED_ROUTE_ATTRIBUTES or attribute.kind in other
  }
  let other_attributes = other |> sort
  {
    family,
    table: entry.table,
    destination: entry.destination,
    prefix: entry.destination_prefix_length,
    source: entry.source,
    source_prefix: entry.source_prefix_length,
    preferred_source: entry.preferred_source,
    tos: entry.tos,
    priority: entry.priority ?? 0,
    protocol: entry.protocol,
    scope: entry.scope,
    route_type: entry.route_type,
    flags: entry.flags,
    hops,
    other_attributes,
  }
}

## The state a route-netlink dump describes. A failed socket, link, address
## or route dump, or one object of those that did not decode, is the error of
## the first such issue. A policy-rule or counter issue does not block it,
## because `State` holds neither; an incomplete dump that names no issue at
## all is `Malformed`.
export pure project(dump: LinuxNetworkDump) -> Result[State, ObserveFailure] {
  for issue in dump.issues {
    return Err(issue_failure(issue)) when issue.object in OBSERVED_OBJECTS
  }
  if dump.state != "complete" and dump.issues.is_empty() {
    fail .Malformed(object: "dump", message: f"a {dump.state} dump reports no issue")
  }
  let links = [link_record(entry)? for entry in dump.links]
  var addresses: List[Address] = []
  var other_addresses: List[Str] = []
  for entry in dump.addresses {
    if let family = family_of(entry.family) {
      addresses += [address_record(entry, family)?]
    } else {
      other_addresses += [entry.family]
    }
  }
  var routes: List[Route] = []
  var other_routes: List[Str] = []
  for entry in dump.routes {
    if let family = family_of(entry.family) {
      routes += [route_record(entry, family)]
    } else {
      other_routes += [entry.family]
    }
  }
  Ok({links, addresses, routes, unmodeled: count_unmodeled("address", other_addresses).extend(count_unmodeled("route", other_routes))})
}

## Observes the caller's network namespace. Hosts without route netlink are
## `Unsupported` with object `backend`.
export proc observe() [process, env, error] -> Result[State, ObserveFailure] {
  match linux.network_dump() {
    Ok(dump) => project(dump)
    Err(failure) => {
      let linux_host = if let Ok(host) = system.uname() { host.sysname == "Linux" } else { true }
      return Err(ObserveFailure.Unsupported(object: "backend", message: "route netlink is only available on Linux"), cause: failure) unless linux_host
      Err(ObserveFailure.Failed(object: "socket", errno: failure.errno, message: failure.message), cause: failure)
    }
  }
}

pure attribute(kind: Int, data: Bytes) -> Result[Bytes, Error] {
  let length = 4 + data.len()
  Ok(bytes.concat([bytes.pack_le(length, 2)?, bytes.pack_le(kind, 2)?, data, bytes.zero((4 - length % 4) % 4)?]))
}

pure family_number(family: Family) -> Int {
  let c = linux.net_constants()
  if family == .Inet { c.AF_INET } else { c.AF_INET6 }
}

pure address_attribute(kind: Int, family: Family, text: Str) -> Result[Bytes, Error] {
  let raw = if family == .Inet { nettools.ipv4_parse(text) } else { nettools.ipv6_parse(text) }
  guard let value = raw else { fail f"{text} is not an {family} address" }
  attribute(kind, value)
}

pure int_attribute(kind: Int, value: Int) -> Result[Bytes, Error] {
  attribute(kind, bytes.pack_le(value, 4)?)
}

pure link_request(ifindex: Int, up: Bool) -> Result[Request, Error] {
  let c = linux.net_constants()
  let payload = bytes.concat([bytes.zero(4)?, bytes.pack_le(ifindex, 4)?, bytes.pack_le(if up { c.IFF_UP } else { 0 }, 4)?, bytes.pack_le(c.IFF_UP, 4)?])
  Ok({message_type: c.RTM_NEWLINK, flags: c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK), payload})
}

pure address_request(item: Address, add: Bool) -> Result[Request, Error] {
  let c = linux.net_constants()
  let header = bytes.concat([bytes.from_ints([family_number(item.family), item.prefix, 0, item.scope])?, bytes.pack_le(item.ifindex, 4)?])
  let peer = address_attribute(c.IFA_ADDRESS, item.family, item.peer ?? item.local)?
  let local = address_attribute(c.IFA_LOCAL, item.family, item.local)?
  var announced = b""
  if let broadcast = item.broadcast {
    if add { announced = address_attribute(c.IFA_BROADCAST, item.family, broadcast)? }
  }
  let base = c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK)
  Ok({
    message_type: if add { c.RTM_NEWADDR } else { c.RTM_DELADDR },
    flags: if add { base.bit_or(c.NLM_F_CREATE).bit_or(c.NLM_F_EXCL) } else { base },
    payload: bytes.concat([header, peer, local, announced]),
  })
}

pure route_request(item: Route, add: Bool) -> Result[Request, Error] {
  let c = linux.net_constants()
  fail "a multipath route cannot be changed through this library" when item.hops.len() != 1
  fail f"route attribute kinds {[f"{kind}" for kind in item.other_attributes].join(",")} cannot be encoded" when add and !item.other_attributes.is_empty()
  let hop = item.hops[0]
  let header_table = if item.table > 255 { 0 } else { item.table }
  let header = bytes.concat([bytes.from_ints([family_number(item.family), item.prefix, item.source_prefix, item.tos, header_table, item.protocol, item.scope, item.route_type])?, bytes.zero(4)?])
  var attributes: List[Bytes] = []
  if item.prefix > 0 {
    guard let destination = item.destination else { fail f"a /{item.prefix} route needs a destination" }
    attributes += [address_attribute(c.RTA_DST, item.family, destination)?]
  }
  if item.source_prefix > 0 {
    guard let source = item.source else { fail f"a /{item.source_prefix} source selector needs a source" }
    attributes += [address_attribute(RTA_SRC, item.family, source)?]
  }
  if let preferred = item.preferred_source {
    attributes += [address_attribute(c.RTA_PREFSRC, item.family, preferred)?]
  }
  if let gateway = hop.gateway {
    attributes += [address_attribute(c.RTA_GATEWAY, item.family, gateway)?]
  }
  if let ifindex = hop.ifindex {
    attributes += [int_attribute(c.RTA_OIF, ifindex)?]
  }
  if item.priority != 0 {
    attributes += [int_attribute(c.RTA_PRIORITY, item.priority)?]
  }
  attributes += [int_attribute(c.RTA_TABLE, item.table)?]
  let base = c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK)
  Ok({
    message_type: if add { c.RTM_NEWROUTE } else { c.RTM_DELROUTE },
    flags: if add { base.bit_or(c.NLM_F_CREATE).bit_or(c.NLM_F_EXCL) } else { base },
    payload: bytes.concat([header, bytes.concat(attributes)]),
  })
}

## The route-netlink request for one change. An address addition sends its
## broadcast address when it has one; `scope` is sent and `flags` are not.
## A route change sends the destination, source selector, preferred source,
## its single hop, a non-zero priority and the table, with the record's
## protocol, scope and type, so a removal deletes exactly the observed route.
## An addition is exclusive and fails with EEXIST when the object exists.
export pure request(change: Change) -> Result[Request, ChangeFailure] {
  let encoded = match change {
    LinkUp(ifindex) => link_request(ifindex, true)
    LinkDown(ifindex) => link_request(ifindex, false)
    AddAddress(item) => address_request(item, true)
    RemoveAddress(item) => address_request(item, false)
    AddRoute(item) => route_request(item, true)
    RemoveRoute(item) => route_request(item, false)
  }
  match encoded {
    Ok(value) => Ok(value)
    Err(failure) => Err(ChangeFailure.Unencodable(failure.message), cause: failure)
  }
}

## Makes one change and waits for the kernel's verdict.
export proc apply(change: Change) [process, net, error] -> Result[Unit, ChangeFailure] {
  let encoded_request = request(change)?
  match linux.netlink_route_request(encoded_request.message_type, encoded_request.flags, encoded_request.payload, CHANGE_TIMEOUT_MS) {
    Ok(ack) => {
      fail .Refused(errno: ack.errno, message: errno_text(ack.errno)) when ack.errno != 0
      Ok()
    }
    Err(failure) => Err(ChangeFailure.Transport(failure.message), cause: failure)
  }
}

## Opens a notification socket for link, IPv4 and IPv6 address and route
## changes. The caller owns and closes the descriptor.
export proc subscribe() [process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let groups = c.RTMGRP_LINK.bit_or(c.RTMGRP_IPV4_IFADDR).bit_or(c.RTMGRP_IPV4_ROUTE).bit_or(c.RTMGRP_IPV6_IFADDR).bit_or(c.RTMGRP_IPV6_ROUTE)
  linux.netlink_open(c.NETLINK_ROUTE, groups)
}

## The message types in one notification datagram, or the reason it cannot
## be trusted: a header or length that does not fit the datagram, or a
## kernel overrun report.
export pure notice_types(datagram: Bytes) -> Result[List[Int], Error] {
  let c = linux.net_constants()
  var offset = 0
  var kinds: List[Int] = []
  while offset < datagram.len() {
    fail f"notification header truncated at byte {offset}" when datagram.len() - offset < NLMSG_HEADER
    let length = bytes.unpack_le(datagram, 4, offset) ?? 0
    fail f"notification length {length} at byte {offset} does not fit the datagram" when length < NLMSG_HEADER or length > datagram.len() - offset
    let kind = bytes.unpack_le(datagram, 2, offset + 4) ?? 0
    fail "kernel reported a notification overrun" when kind == c.NLMSG_OVERRUN
    kinds += [kind]
    offset += (length + 3) / 4 * 4
  }
  Ok(kinds)
}

## Reads every pending notification without blocking. `Lost` means the
## kernel dropped notifications (ENOBUFS) or a datagram did not decode, and
## the caller's last observation can no longer be trusted.
export proc drain(fd: Int) [process, net, error] -> Result[Notices, Error] {
  var notices: Notices = .Quiet
  var reads = 0
  while reads < MAX_NOTICE_READS and !unix.poll_fd(fd, ["readable"], 0)?.is_empty() {
    reads += 1
    match unix.read_fd(fd, NOTICE_BYTES) {
      Ok(datagram) => {
        match notice_types(datagram) {
          Ok(kinds) => {
            notices = .Changed unless kinds.is_empty()
          }
          Err(reason) => return Ok(.Lost(reason.message))
        }
      }
      Err(failure) => {
        return Ok(.Lost("kernel dropped notifications (ENOBUFS)")) when (failure.errno ?? 0) == ENOBUFS
        return Err(failure) when (failure.errno ?? 0) not in EAGAIN_CODES
      }
    }
  }
  Ok(notices)
}
