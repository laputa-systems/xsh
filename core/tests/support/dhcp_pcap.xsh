##! Test support: the classic pcap capture format, a frame dissector for the
##! Ethernet, ARP, IPv4 and UDP layers the DHCP tests check, and a recorder
##! that appends the frames of one AF_PACKET socket to a capture file. The
##! dissector reads frames independently of the wire library, so a test can
##! compare what crossed a link with what the client believes it sent.
use lib.dhcp_arp as arp
use lib.dhcp_wire as wire

## One captured frame with its capture time in microseconds.
export type Captured = {at_us: Int, frame: Bytes}

## The Ethernet header and, when present, the layers above it.
export type Frame = {
  destination_mac: Bytes,
  source_mac: Bytes,
  ethertype: Int,
  arp: arp.ArpPacket?,
  ip: IpHeader?,
  udp: UdpDatagram?,
}

## The fields of an IPv4 header the tests compare.
export type IpHeader = {source: Int, destination: Int, protocol: Int, length: Int, checksum_ok: Bool}

## A UDP datagram and its payload.
export type UdpDatagram = {source_port: Int, destination_port: Int, payload: Bytes}

const MAGIC = 2712847316
const SNAPLEN = 65535

## The 24-byte global header of a little-endian microsecond Ethernet capture.
export pure header() -> Bytes {
  bytes.concat([bytes.pack_le(MAGIC, 4) ?? b"", bytes.pack_le(2, 2) ?? b"", bytes.pack_le(4, 2) ?? b"", bytes.zero(8) ?? b"", bytes.pack_le(SNAPLEN, 4) ?? b"", bytes.pack_le(1, 4) ?? b""])
}

## One record: the capture time, the lengths and the frame.
export pure record(at_us: Int, frame: Bytes) -> Bytes {
  bytes.concat([bytes.pack_le(at_us / 1000000, 4) ?? b"", bytes.pack_le(at_us % 1000000, 4) ?? b"", bytes.pack_le(frame.len(), 4) ?? b"", bytes.pack_le(frame.len(), 4) ?? b"", frame])
}

## Reads a capture file: the frames in order, or an error for a bad magic
## number, an unsupported link type or a truncated record.
export pure parse(data: Bytes) -> Result[List[Captured], Error] {
  if data.len() < 24 or bytes.unpack_le(data, 4, 0)? != MAGIC or bytes.unpack_le(data, 4, 20)? != 1 {
    return Err(error.failure("not a little-endian Ethernet pcap file"))
  }
  var offset = 24
  let found = collect {
    while offset < data.len() {
      return Err(error.failure("truncated pcap record header")) when offset + 16 > data.len()
      let seconds = bytes.unpack_le(data, 4, offset)?
      let micros = bytes.unpack_le(data, 4, offset + 4)?
      let length = bytes.unpack_le(data, 4, offset + 8)?
      return Err(error.failure("truncated pcap record")) when offset + 16 + length > data.len()
      yield {at_us: seconds * 1000000 + micros, frame: data[offset + 16..offset + 16 + length]}
      offset += 16 + length
    }
  }
  Ok(found)
}

pure checksum_ok(span: Bytes) -> Bool {
  var sum = 0
  var index = 0
  while index + 1 < span.len() {
    sum += (span.byte_at(index) ?? 0) * 256 + (span.byte_at(index + 1) ?? 0)
    index += 2
  }
  while sum > 65535 {
    sum = sum.bit_and(65535) + sum / 65536
  }
  sum == 65535
}

## Splits a frame into the layers the DHCP tests check.
export pure dissect(frame: Bytes) -> Frame {
  let empty = Frame(destination_mac: b"", source_mac: b"", ethertype: 0, arp: null, ip: null, udp: null)
  return empty when frame.len() < 14
  let ethertype = bytes.unpack_be(frame, 2, 12) ?? 0
  let base = Frame(destination_mac: frame[..6], source_mac: frame[6..12], ethertype:, arp: null, ip: null, udp: null)
  return {...base, arp: arp.parse(frame)} when ethertype == 2054
  return base unless ethertype == 2048 and frame.len() >= 34
  let ihl = (frame.byte_at(14) ?? 0).bit_and(15) * 4
  return base when ihl < 20 or frame.len() < 14 + ihl
  let protocol = frame.byte_at(23) ?? 0
  let ip = IpHeader(
    source: bytes.unpack_be(frame, 4, 26) ?? 0,
    destination: bytes.unpack_be(frame, 4, 30) ?? 0,
    protocol:,
    length: bytes.unpack_be(frame, 2, 16) ?? 0,
    checksum_ok: checksum_ok(frame[14..14 + ihl]),
  )
  return {...base, ip: ip} unless protocol == 17 and frame.len() >= 14 + ihl + 8
  let udp_start = 14 + ihl
  let udp_length = bytes.unpack_be(frame, 2, udp_start + 4) ?? 0
  let payload_end = if udp_start + udp_length <= frame.len() { udp_start + udp_length } else { frame.len() }
  {
    ...base,
    ip: ip,
    udp: UdpDatagram(
      source_port: bytes.unpack_be(frame, 2, udp_start) ?? 0,
      destination_port: bytes.unpack_be(frame, 2, udp_start + 2) ?? 0,
      payload: frame[udp_start + 8..payload_end],
    ),
  }
}

pure be16(value: Int) -> Bytes {
  bytes.pack_be(value, 2) ?? b""
}

pure be32(value: Int) -> Bytes {
  bytes.pack_be(value, 4) ?? b""
}

## The IPv4 header checksum field value for `header` whose checksum bytes are zero.
export pure header_checksum(header: Bytes) -> Int {
  var sum = 0
  var index = 0
  while index + 1 < header.len() {
    sum += (header.byte_at(index) ?? 0) * 256 + (header.byte_at(index + 1) ?? 0)
    index += 2
  }
  while sum > 65535 {
    sum = sum.bit_and(65535) + sum / 65536
  }
  65535 - sum
}

## Builds an Ethernet frame carrying an IPv4 packet with a correct header
## checksum and a UDP datagram with a zero checksum, for tests that need a
## frame they control byte for byte.
export pure udp_frame(destination_mac: Bytes, source_mac: Bytes, source: Int, destination: Int, source_port: Int, destination_port: Int, payload: Bytes) -> Bytes {
  let udp = bytes.concat([be16(source_port), be16(destination_port), be16(8 + payload.len()), be16(0), payload])
  let unchecked = bytes.concat([b"\x45\x00", be16(20 + udp.len()), be16(0), be16(0), b"\x40\x11", be16(0), be32(source), be32(destination)])
  let ip_header = bytes.concat([unchecked[..10], be16(header_checksum(unchecked)), unchecked[12..]])
  bytes.concat([destination_mac, source_mac, b"\x08\x00", ip_header, udp])
}

## One decoded DHCP message with the frame it travelled in.
export type Exchange = {at_us: Int, frame: Frame, message: wire.Message}

## The DHCP messages in a capture, in order, with the dissected frame each came
## from. Datagrams to or from the DHCP ports that do not decode are skipped.
export pure dhcp_messages(captured: List[Captured]) -> List[Exchange] {
  collect {
    for entry in captured {
      let dissected = dissect(entry.frame)
      if let datagram = dissected.udp {
        if datagram.source_port in [67, 68] and datagram.destination_port in [67, 68] {
          if let Ok(message) = wire.decode(datagram.payload) {
            yield {at_us: entry.at_us, frame: dissected, message: message}
          }
        }
      }
    }
  }
}

## Appends every frame the interface `name` sees to the capture file `output`
## until the process is killed. Each record is written whole before the next
## read, so a killed recorder leaves a parseable file.
export proc record_interface(name: Str, output: Path) [fs, process, net, time, error] {
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_PACKET, c.SOCK_RAW, 768)?
  defer unix.close_fd(socket)
  linux.setsockopt_bytes(socket, c.SOL_SOCKET, c.SO_BINDTODEVICE, bytes.from_text(name))
  output.write(header())
  let sink = unix.open_fd(output, write: true, flags: ["append"])?
  defer unix.close_fd(sink)
  let started = time.boottime()?
  while true {
    if unix.poll_fd(socket, ["readable"], 200)? == ["readable"] {
      let frame = unix.read_fd(socket, 4096)?
      let at = (time.boottime()? - started) / 1000
      let _ = unix.write_fd(sink, record(at, frame))?
    }
  }
}
