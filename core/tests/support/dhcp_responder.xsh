##! Test support: the decisions of a scripted DHCPv4 server, free of sockets and
##! clocks. Given a decoded client request, the rules of a script and the count
##! of requests of that kind so far, it names the rule that answers and builds
##! each reply datagram, including the faulty ones: a transaction or client
##! that is not the request's, a destination other than broadcast, a truncated,
##! corrupt or garbage datagram. The socket server and the in-process fake
##! world both use it.
use lib.dhcp_wire as wire
use tests.support.dhcp_types as types

## How many requests of each kind a server has seen.
export type Counts = {discover: Int, request: Int, decline: Int, release: Int, inform: Int}

## A decoded client request and where it came from.
export type Request = {kind: Str, message: wire.Message, source: Str, source_port: Int}

## The request name of a DHCP message type.
export pure kind_name(kind: Int) -> Str {
  match kind {
    1 => "discover"
    3 => "request"
    4 => "decline"
    7 => "release"
    8 => "inform"
    else => "other"
  }
}

## The counts after one more request of `kind`.
export pure bump(counts: Counts, kind: Str) -> Counts {
  match kind {
    "discover" => {...counts, discover: counts.discover + 1}
    "request" => {...counts, request: counts.request + 1}
    "decline" => {...counts, decline: counts.decline + 1}
    "release" => {...counts, release: counts.release + 1}
    "inform" => {...counts, inform: counts.inform + 1}
    else => counts
  }
}

## How many requests of `kind` have been counted.
export pure seen(counts: Counts, kind: Str) -> Int {
  match kind {
    "discover" => counts.discover
    "request" => counts.request
    "decline" => counts.decline
    "release" => counts.release
    "inform" => counts.inform
    else => 0
  }
}

pure rule_matches(rule: types.Rule, kind: Str, occurrence: Int) -> Bool {
  return false unless rule.on == kind
  if let only = rule.nth {
    return occurrence == only
  }
  if let first = rule.from {
    return occurrence >= first
  }
  true
}

## The first rule that answers the `occurrence`th request of `kind`.
export pure pick(rules: List[types.Rule], kind: Str, occurrence: Int) -> types.Rule? {
  for rule in rules {
    return rule when rule_matches(rule, kind, occurrence)
  }
  null
}

pure address_bytes(text: Str) -> Bytes {
  bytes.pack_be(wire.ip_parse(text) ?? 0, 4) ?? b""
}

pure reply_kind(name: Str) -> Int {
  match name {
    "offer" => wire.OFFER
    "ack" => wire.ACK
    else => wire.NAK
  }
}

pure classless_entry(text: Str) -> Bytes {
  let parts = text.split(" ")
  let network = parts[0].split("/")
  let prefix = network[1].parse_int() ?? 0
  let destination = bytes.pack_be(wire.ip_parse(network[0]) ?? 0, 4) ?? b""
  bytes.concat([bytes.from_ints([prefix]) ?? b"", destination[..(prefix + 7) / 8], address_bytes(parts[1])])
}

## The datagram a reply specification produces for a request.
export pure reply_datagram(reply: types.Reply, request: wire.Message) -> Bytes {
  let xid = if reply.xid == "other" { request.xid + 1 } else { request.xid }
  let chaddr = if reply.chaddr == "other" { b"\x02\xee\xee\xee\xee\xee" } else { request.chaddr }
  let yiaddr = if reply.kind == "nak" { 0 } else { wire.ip_parse(reply.address) ?? 0 }
  let header = {op: 2, xid: xid, secs: 0, broadcast: request.broadcast, ciaddr: 0, yiaddr: yiaddr, siaddr: 0, giaddr: 0, chaddr: chaddr}
  let common = [
    wire.option(53, bytes.from_ints([reply_kind(reply.kind)]) ?? b"") ?? b"",
    wire.option(54, address_bytes(reply.server_id)) ?? b"",
  ]
  let granted = if reply.kind == "nak" {
    []
  } else {
    [
      wire.option(51, bytes.pack_be(reply.lease, 4) ?? b"") ?? b"",
      wire.option(1, address_bytes(reply.mask)) ?? b"",
    ]
  }
  let timers = [
    if let seconds = reply.t1 { wire.option(58, bytes.pack_be(seconds, 4) ?? b"") ?? b"" } else { b"" },
    if let seconds = reply.t2 { wire.option(59, bytes.pack_be(seconds, 4) ?? b"") ?? b"" } else { b"" },
  ]
  let routers = if reply.routers.is_empty() or reply.kind == "nak" { [] } else { [wire.option(3, bytes.concat([address_bytes(router) for router in reply.routers])) ?? b""] }
  let classless = if reply.classless.is_empty() or reply.kind == "nak" { [] } else { [wire.option(121, bytes.concat([classless_entry(route) for route in reply.classless])) ?? b""] }
  let servers = if reply.dns.is_empty() or reply.kind == "nak" { [] } else { [wire.option(6, bytes.concat([address_bytes(server) for server in reply.dns])) ?? b""] }
  let domain = if let name = reply.domain { [wire.option(15, bytes.from_text(name)) ?? b""] } else { [] }
  let options = [@common, @granted, @timers, @routers, @classless, @servers, @domain]
  corrupt(wire.encode_message(header, options) ?? b"", reply.corrupt)
}

pure corrupt(datagram: Bytes, how: Str) -> Bytes {
  match how {
    "truncate" => datagram[..120]
    "cookie" => bytes.concat([datagram[..236], b"\x00\x00\x00\x00", datagram[240..]])
    "option_length" => bytes.concat([datagram[..240], b"\x35\xf0\x02", datagram[243..]])
    "no_end" => bytes.concat([datagram[..240], b"\x35\x01\x02\x36\x04\x01\x02\x03\x04", bytes.zero(60) ?? b""])
    "garbage" => bytes.from_text("this is not a DHCP message at all")
    else => datagram
  }
}

## The address a reply is sent to.
export pure destination(reply: types.Reply, request: Request) -> Str {
  match reply.dest {
    "ciaddr" => wire.ip_text(request.message.ciaddr)
    "yiaddr" => reply.address
    "source" => request.source
    else => "255.255.255.255"
  }
}

