##! Test support: a scripted DHCPv4 server built on the production wire
##! library. It answers requests according to a JSON script of rules, can send
##! duplicate, out-of-order, unrelated, malformed and delayed replies or stay
##! silent, and appends one JSON line per request it receives to a log. It is
##! the controlled counterpart of dnsmasq: dnsmasq checks the client against an
##! independent implementation, this server checks it against faults no real
##! server produces on demand.
use lib.dhcp_wire as wire
use tests.support.dhcp_types as types




type Counts = {discover: Int, request: Int, decline: Int, release: Int, inform: Int}

type Request = {kind: Str, message: wire.Message, source: Str, source_port: Int}

pure kind_name(kind: Int) -> Str {
  match kind {
    1 => "discover"
    3 => "request"
    4 => "decline"
    7 => "release"
    8 => "inform"
    else => "other"
  }
}

pure bump(counts: Counts, kind: Str) -> Counts {
  match kind {
    "discover" => {...counts, discover: counts.discover + 1}
    "request" => {...counts, request: counts.request + 1}
    "decline" => {...counts, decline: counts.decline + 1}
    "release" => {...counts, release: counts.release + 1}
    "inform" => {...counts, inform: counts.inform + 1}
    else => counts
  }
}

pure seen(counts: Counts, kind: Str) -> Int {
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

pure pick(rules: List[types.Rule], kind: Str, occurrence: Int) -> types.Rule? {
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

pure reply_datagram(reply: types.Reply, request: wire.Message) -> Bytes {
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
  let servers = if reply.dns.is_empty() or reply.kind == "nak" { [] } else { [wire.option(6, bytes.concat([address_bytes(server) for server in reply.dns])) ?? b""] }
  let domain = if let name = reply.domain { [wire.option(15, bytes.from_text(name)) ?? b""] } else { [] }
  let options = [@common, @granted, @timers, @routers, @servers, @domain]
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

pure destination(reply: types.Reply, request: Request) -> Str {
  match reply.dest {
    "ciaddr" => wire.ip_text(request.message.ciaddr)
    "yiaddr" => reply.address
    "source" => request.source
    else => "255.255.255.255"
  }
}

pure log_line(request: Request, occurrence: Int, at_ms: Int) -> Str {
  let message = request.message
  json.encode(
    {
      at_ms: at_ms,
      kind: request.kind,
      occurrence: occurrence,
      xid: message.xid,
      ciaddr: wire.ip_text(message.ciaddr),
      broadcast_flag: message.broadcast,
      requested_ip: if let address = message.requested_ip { wire.ip_text(address) } else { null },
      server_id: if let address = message.server_id { wire.ip_text(address) } else { null },
      client_id: if let id = message.client_id { id.base64() } else { null },
      parameters: message.parameter_list,
      source: request.source,
      source_port: request.source_port,
    },
  ) ?? ""
}

proc send_reply(socket: Int, reply: types.Reply, request: Request) [process, net, time, error] {
  if reply.delay_ms > 0 {
    time.sleep(reply.delay_ms * 1ms)
  }
  repeat max_of(reply.repeat, 1) times {
    let _ = linux.sendto(socket, reply_datagram(reply, request.message), {family: "inet", address: destination(reply, request), port: wire.CLIENT_PORT})?
  }
}

pure max_of(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

proc serve(socket: Int, script: types.Script, sink: Int, started: Int) [process, net, time, error] {
  var counts = Counts(discover: 0, request: 0, decline: 0, release: 0, inform: 0)
  while true {
    match linux.recvfrom(socket, 2048) {
      Ok(received) => {
        let at = (time.boottime()? - started) / 1000000
        if let Ok(message) = wire.decode(received.data) {
          let kind = kind_name(message.kind)
          counts = bump(counts, kind)
          let request = Request(kind:, message:, source: received.address.address, source_port: received.address.port)
          let occurrence = seen(counts, kind)
          let line = log_line(request, occurrence, at)
          let _ = unix.write_fd(sink, bytes.from_text(f"{line}\n"))?
          if let rule = pick(script.rules, kind, occurrence) {
            for reply in rule.replies {
              send_reply(socket, reply, request)
            }
          }
        } else {
          let line = json.encode({at_ms: at, kind: "malformed", bytes: received.data.len()}) ?? ""
          let _ = unix.write_fd(sink, bytes.from_text(f"{line}\n"))?
        }
      }
      Err(failure) => {
        return Err(failure) when (failure.errno ?? 0) not in [11, 35, 4]
      }
    }
  }
}

proc main(...argv: List[Str]) [fs, process, net, time, error] {
  let c = linux.net_constants()
  let interface = argv[0]
  let script = json.read(fp"{argv[1]}")?.require(types.Script)?
  let log = fp"{argv[2]}"
  log.write("")
  let sink = unix.open_fd(log, write: true, flags: ["append"])?
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  linux.setsockopt_int(socket, c.SOL_SOCKET, c.SO_REUSEADDR, 1)
  linux.setsockopt_int(socket, c.SOL_SOCKET, c.SO_BROADCAST, 1)
  linux.setsockopt_bytes(socket, c.SOL_SOCKET, c.SO_BINDTODEVICE, bytes.from_text(interface))
  linux.bind(socket, {family: "inet", address: "0.0.0.0", port: wire.SERVER_PORT})
  linux.set_socket_timeout(socket, c.SO_RCVTIMEO, 200)
  serve(socket, script, sink, time.boottime()?)
}
