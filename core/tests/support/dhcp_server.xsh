##! Test support: a scripted DHCPv4 server built on the production wire
##! library. It answers requests according to a JSON script of rules, can send
##! duplicate, out-of-order, unrelated, malformed and delayed replies or stay
##! silent, and appends one JSON line per request it receives to a log. It is
##! the controlled counterpart of dnsmasq: dnsmasq checks the client against an
##! independent implementation, this server checks it against faults no real
##! server produces on demand.
use lib.dhcp_wire as wire
use tests.support.dhcp_responder as responder
use tests.support.dhcp_types as types

pure log_line(request: responder.Request, occurrence: Int, at_ms: Int) -> Str {
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

proc send_reply(socket: Int, reply: types.Reply, request: responder.Request) [process, net, time, error] {
  if reply.delay_ms > 0 {
    time.sleep(reply.delay_ms * 1ms)
  }
  repeat max_of(reply.repeat, 1) times {
    let _ = linux.sendto(socket, responder.reply_datagram(reply, request.message), {family: "inet", address: responder.destination(reply, request), port: wire.CLIENT_PORT})?
  }
}

pure max_of(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

proc serve(socket: Int, script: types.Script, sink: Int, started: Int) [process, net, time, error] {
  var counts = responder.Counts(discover: 0, request: 0, decline: 0, release: 0, inform: 0)
  while true {
    match linux.recvfrom(socket, 2048) {
      Ok(received) => {
        let at = (time.boottime()? - started) / 1000000
        if let Ok(message) = wire.decode(received.data) {
          let kind = responder.kind_name(message.kind)
          counts = responder.bump(counts, kind)
          let request = responder.Request(kind:, message:, source: received.address.address, source_port: received.address.port)
          let occurrence = responder.seen(counts, kind)
          let line = log_line(request, occurrence, at)
          let _ = unix.write_fd(sink, bytes.from_text(f"{line}\n"))?
          if let rule = responder.pick(script.rules, kind, occurrence) {
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
