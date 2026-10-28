##! ARP frames for the address-conflict check a DHCP client makes before it
##! configures a leased address (RFC 5227): the probe that asks whether anyone
##! uses the address, the announcement that claims it, and the test that tells
##! a conflicting frame from an unrelated one. Frames are complete Ethernet
##! frames so one AF_PACKET socket sends and receives them.

## The fields of an ARP request or reply for Ethernet and IPv4.
export type ArpPacket = {operation: Int, sender_mac: Bytes, sender_ip: Int, target_mac: Bytes, target_ip: Int}

## The number of probes sent before an address is declared free.
export const PROBE_COUNT = 3
## The number of gratuitous announcements sent after the address is claimed.
export const ANNOUNCE_COUNT = 2
## The shortest wait between probes, in milliseconds.
export const PROBE_MIN_MS = 1000
## The longest wait between probes, in milliseconds.
export const PROBE_MAX_MS = 2000
## The wait between announcements, in milliseconds.
export const ANNOUNCE_INTERVAL_MS = 2000
## The wait after the last probe for a conflicting answer, in milliseconds.
export const ANNOUNCE_WAIT_MS = 2000

const ETHERNET_BROADCAST = b"\xff\xff\xff\xff\xff\xff"
const ETHERTYPE_ARP = 2054
const ETHERTYPE_IPV4 = 2048
const ARP_REQUEST = 1
const ARP_REPLY = 2
const FRAME_HEADER = 14
const ARP_LENGTH = 28
const MINIMUM_FRAME = 60

pure be32(value: Int) -> Result[Bytes, Error] {
  bytes.pack_be(value, 4)
}

pure frame(source: Bytes, operation: Int, sender_ip: Int, target_ip: Int) -> Result[Bytes, Error] {
  return Err(error.failure("an ARP frame needs a six-byte hardware address")) when source.len() != 6
  let body = bytes.concat(
    [
      ETHERNET_BROADCAST,
      source,
      bytes.pack_be(ETHERTYPE_ARP, 2)?,
      bytes.pack_be(1, 2)?,
      bytes.pack_be(ETHERTYPE_IPV4, 2)?,
      bytes.from_ints([6, 4])?,
      bytes.pack_be(operation, 2)?,
      source,
      be32(sender_ip)?,
      bytes.zero(6)?,
      be32(target_ip)?,
    ],
  )
  Ok(bytes.concat([body, bytes.zero(MINIMUM_FRAME - body.len())?]))
}

## The probe for `candidate`: sender address 0.0.0.0, so a host that already
## uses the address is not taught our mapping.
export pure probe(mac: Bytes, candidate: Int) -> Result[Bytes, Error] {
  frame(mac, ARP_REQUEST, 0, candidate)
}

## The announcement of `address`: sender and target both name it, which makes
## neighbours refresh their caches.
export pure announce(mac: Bytes, address: Int) -> Result[Bytes, Error] {
  frame(mac, ARP_REQUEST, address, address)
}

## Decodes an Ethernet frame as an ARP packet for Ethernet and IPv4, or null
## for anything else, including a truncated frame.
export pure parse(data: Bytes) -> ArpPacket? {
  return null when data.len() < FRAME_HEADER + ARP_LENGTH
  return null when (bytes.unpack_be(data, 2, 12) ?? 0) != ETHERTYPE_ARP
  return null when (bytes.unpack_be(data, 2, 14) ?? 0) != 1 or (bytes.unpack_be(data, 2, 16) ?? 0) != ETHERTYPE_IPV4
  return null when data.byte_at(18) != 6 or data.byte_at(19) != 4
  let operation = bytes.unpack_be(data, 2, 20) ?? 0
  return null when operation != ARP_REQUEST and operation != ARP_REPLY
  {
    operation,
    sender_mac: data[22..28],
    sender_ip: bytes.unpack_be(data, 4, 28) ?? 0,
    target_mac: data[32..38],
    target_ip: bytes.unpack_be(data, 4, 38) ?? 0,
  }
}

## True when `data` shows another host using `candidate` while we probe it: a
## packet from a different hardware address that claims the address as its
## sender, or that probes for the same address itself.
export pure conflicts(data: Bytes, mac: Bytes, candidate: Int) -> Bool {
  guard let packet = parse(data) else { return false }
  return false when packet.sender_mac == mac
  return true when packet.sender_ip == candidate
  packet.sender_ip == 0 and packet.target_ip == candidate and packet.operation == ARP_REQUEST
}

## The `sockaddr_ll` bytes that bind or address an AF_PACKET socket: the
## family, the protocol in network byte order, the interface index, no
## hardware type or packet type and, for a destination, the six-byte hardware
## address. Little-endian hosts only, which are the only hosts this runs on.
export pure packet_address(ifindex: Int, protocol: Int, destination: Bytes) -> Result[Bytes, Error] {
  if destination.len() > 8 {
    return Err(error.failure("a link-layer address is at most eight bytes"))
  }
  Ok(
    bytes.concat(
      [
        bytes.pack_le(17, 2)?,
        bytes.pack_be(protocol, 2)?,
        bytes.pack_le(ifindex, 4)?,
        bytes.pack_le(0, 2)?,
        bytes.from_ints([0, destination.len()])?,
        destination,
        bytes.zero(8 - destination.len())?,
      ],
    ),
  )
}

## The protocol argument of `linux.socket` for ARP: the kernel takes it in
## network byte order.
export pure arp_socket_protocol() -> Int {
  ETHERTYPE_ARP % 256 * 256 + ETHERTYPE_ARP / 256
}
