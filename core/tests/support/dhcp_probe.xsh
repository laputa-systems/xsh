##! Test support: the smallest possible DHCP client. It sends one DISCOVER,
##! answers the first valid OFFER with a selecting REQUEST and writes the
##! resulting messages as JSON. It exists to check the wire library against a
##! real server, and the server end against a plain client, before the full
##! client is involved. Arguments: interface, MAC, output file, xid.
use lib.dhcp_arp as arp
use lib.dhcp_frame as frame_codec
use lib.dhcp_wire as wire
use lib.nettools_fixture as fixture

type Seen = {kind: Int, xid: Int, address: Str, server: Str, lease: Int, renew: Int?, rebind: Int?, mask: Str, routers: List[Str], classless: List[Str], dns: List[Str], domain: Str?, broadcast: Bool}

pure summary(message: wire.Message) -> Seen {
  {
    kind: message.kind,
    xid: message.xid,
    address: wire.ip_text(message.yiaddr),
    server: wire.ip_text(message.server_id ?? 0),
    lease: message.lease_secs ?? 0,
    renew: message.renewal_secs,
    rebind: message.rebinding_secs,
    mask: wire.ip_text(message.subnet_mask ?? 0),
    routers: [wire.ip_text(router) for router in message.routers],
    classless: [f"{wire.ip_text(route.destination)}/{route.prefix} {wire.ip_text(route.gateway)}" for route in message.classless_routes],
    dns: [wire.ip_text(server) for server in message.dns],
    domain: message.domain,
    broadcast: message.broadcast,
  }
}

proc receive(packets: Int, mac: Bytes, identity: wire.Identity, xid: Int, kind: Int, attempts: Int) [process, net, error] -> Result[wire.Message?] {
  var left = attempts
  while left > 0 {
    left -= 1
    if unix.poll_fd(packets, ["readable"], 1000)? == ["readable"] {
      let frame = unix.read_fd(packets, 2048)?
      match frame_codec.server_datagram(frame, mac) {
        Ok(found) => {
          match wire.accept_reply(found.payload, identity, xid) {
            Ok(message) => {
              return Ok(message) when message.kind == kind
            }
            Err(reject) => print f"rejected {wire.reject_reason(reject)}"
          }
        }
        Err(reject) => print f"frame {frame_codec.reject_reason(reject)} {frame.len()}"
      }
    }
  }
  Ok(null)
}

proc main(...argv: List[Str]) [fs, process, net, error] {
  let c = linux.net_constants()
  let interface = argv[0]
  let mac = fixture.mac_bytes(argv[1])
  let xid = argv[3] as Int
  let identity = wire.identity(mac)?
  fixture.link_up(interface)
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  linux.setsockopt_int(socket, c.SOL_SOCKET, c.SO_REUSEADDR, 1)
  linux.setsockopt_int(socket, c.SOL_SOCKET, c.SO_BROADCAST, 1)
  linux.setsockopt_bytes(socket, c.SOL_SOCKET, c.SO_BINDTODEVICE, bytes.from_text(interface))
  linux.bind(socket, {family: "inet", address: "0.0.0.0", port: wire.CLIENT_PORT})
  let packets = linux.socket(c.AF_PACKET, c.SOCK_RAW, 8)?
  defer unix.close_fd(packets)
  linux.bind(packets, arp.packet_address(fixture.index_of(interface), c.ETH_P_IP, b"")?)
  let target = {family: "inet", address: "255.255.255.255", port: wire.SERVER_PORT}
  let _ = linux.sendto(socket, wire.discover(identity, xid, 0)?, target)?
  guard let offer = receive(packets, mac, identity, xid, wire.OFFER, 6)? else {
    fp"{argv[2]}".write("{\"error\":\"no offer\"}")
    exit 3
  }
  let _ = linux.sendto(socket, wire.request_selecting(identity, xid, 0, offer.yiaddr, offer.server_id ?? 0)?, target)?
  guard let ack = receive(packets, mac, identity, xid, wire.ACK, 6)? else {
    fp"{argv[2]}".write("{\"error\":\"no ack\"}")
    exit 4
  }
  fp"{argv[2]}".write(json.encode({offer: summary(offer), ack: summary(ack)})?)
}
