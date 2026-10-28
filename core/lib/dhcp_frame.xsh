##! DHCP over raw Ethernet frames. A client without an address cannot receive
##! its server's reply through a UDP socket on a system that filters by source
##! route, because the reply comes from a network the interface has no route
##! to yet, so the kernel drops it before any socket sees it. The client
##! therefore reads frames from a packet socket and this module does what the
##! IP stack would: it checks the Ethernet, IPv4 and UDP headers, the header
##! checksum, the UDP checksum when one is present, fragmentation and the
##! DHCP ports, and returns the UDP payload for the wire decoder.

## A verified UDP datagram taken out of a frame.
export type Datagram = {source: Int, destination: Int, source_port: Int, destination_port: Int, payload: Bytes}

## Why a frame is not a DHCP server datagram for this client.
export error FrameReject {
  TooShort
  NotIpv4
  BadHeader
  Fragment
  BadChecksum
  NotUdp
  BadLength
  WrongPort
  WrongHardware
}

const ETHERTYPE_IPV4 = 2048
const ETHERNET_HEADER = 14
const IPV4_MINIMUM_HEADER = 20
const UDP_HEADER = 8
const PROTOCOL_UDP = 17

pure word(data: Bytes, offset: Int) -> Int {
  (data.byte_at(offset) ?? 0) * 256 + (data.byte_at(offset + 1) ?? 0)
}

pure sum_words(data: Bytes, start: Int, stop: Int, initial: Int) -> Int {
  var total = initial
  var index = start
  while index + 1 < stop {
    total += word(data, index)
    index += 2
  }
  if index < stop {
    total += (data.byte_at(index) ?? 0) * 256
  }
  total
}

pure fold(total: Int) -> Int {
  var folded = total
  while folded > 65535 {
    folded = folded.bit_and(65535) + folded / 65536
  }
  folded
}

## True when the 16-bit ones'-complement sum over `data[start..stop]` is
## 0xFFFF, which is what a correct IPv4 header checksum yields.
export pure checksum_valid(data: Bytes, start: Int, stop: Int) -> Bool {
  fold(sum_words(data, start, stop, 0)) == 65535
}

## Extracts the UDP datagram of a frame that is addressed to this client: the
## Ethernet destination must be `mac` or the broadcast address. The frame must
## be an unfragmented IPv4 packet with a valid header and a UDP datagram whose
## lengths hold. The ports are the caller's to judge.
export pure datagram(frame: Bytes, mac: Bytes) -> Result[Datagram, FrameReject] {
  fail .TooShort() when frame.len() < ETHERNET_HEADER + IPV4_MINIMUM_HEADER + UDP_HEADER
  fail .WrongHardware() unless frame[..6] == mac or frame[..6] == b"\xff\xff\xff\xff\xff\xff"
  fail .NotIpv4() unless word(frame, 12) == ETHERTYPE_IPV4
  let version_and_length = frame.byte_at(ETHERNET_HEADER) ?? 0
  let header_length = version_and_length.bit_and(15) * 4
  fail .BadHeader() when version_and_length / 16 != 4 or header_length < IPV4_MINIMUM_HEADER or frame.len() < ETHERNET_HEADER + header_length + UDP_HEADER
  fail .BadChecksum() unless checksum_valid(frame, ETHERNET_HEADER, ETHERNET_HEADER + header_length)
  let flags_and_offset = word(frame, ETHERNET_HEADER + 6)
  fail .Fragment() when flags_and_offset.bit_and(8191) != 0 or flags_and_offset.bit_and(8192) != 0
  fail .NotUdp() when (frame.byte_at(ETHERNET_HEADER + 9) ?? 0) != PROTOCOL_UDP
  let total_length = word(frame, ETHERNET_HEADER + 2)
  fail .BadLength() when total_length < header_length + UDP_HEADER or ETHERNET_HEADER + total_length > frame.len()
  let udp_start = ETHERNET_HEADER + header_length
  let udp_length = word(frame, udp_start + 4)
  fail .BadLength() when udp_length < UDP_HEADER or udp_start + udp_length > ETHERNET_HEADER + total_length
  Ok(
    Datagram(
      source: word(frame, ETHERNET_HEADER + 12) * 65536 + word(frame, ETHERNET_HEADER + 14),
      destination: word(frame, ETHERNET_HEADER + 16) * 65536 + word(frame, ETHERNET_HEADER + 18),
      source_port: word(frame, udp_start),
      destination_port: word(frame, udp_start + 2),
      payload: frame[udp_start + UDP_HEADER..udp_start + udp_length],
    ),
  )
}

## The datagram of a frame that is a message from a DHCP server to a DHCP
## client: UDP from port 67 to port 68.
export pure server_datagram(frame: Bytes, mac: Bytes) -> Result[Datagram, FrameReject] {
  let found = datagram(frame, mac)?
  fail .WrongPort() unless found.source_port == 67 and found.destination_port == 68
  Ok(found)
}

## A stable name for a rejection, suitable as a counter key.
export pure reject_reason(reject: FrameReject) -> Str {
  match reject {
    FrameReject.TooShort => "frame_too_short"
    FrameReject.NotIpv4 => "frame_not_ipv4"
    FrameReject.BadHeader => "frame_bad_header"
    FrameReject.Fragment => "frame_fragment"
    FrameReject.BadChecksum => "frame_bad_checksum"
    FrameReject.NotUdp => "frame_not_udp"
    FrameReject.BadLength => "frame_bad_length"
    FrameReject.WrongPort => "frame_wrong_port"
    FrameReject.WrongHardware => "frame_wrong_hardware"
  }
}
