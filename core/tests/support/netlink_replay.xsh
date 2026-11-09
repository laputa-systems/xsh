##! Test support: builds the route-netlink datagrams a `network_replay` file
##! answers `linux.network_dump` and `linux.netlink_route_request` with. Every
##! reply carries the request numbers the replay fake uses: sequence 1 to 4 for
##! the links, addresses, routes and rules dumps, sequence 1 and port 4242 for a
##! change request.

## The port id the replay fake gives every request.
export const PORT = 4242
const NLMSG_ERROR = 2
const NLMSG_DONE = 3
const NLM_F_MULTI = 2

pure message(kind: Int, flags: Int, sequence: Int, payload: Bytes) -> Result[Bytes, Error] {
  let length = 16 + payload.len()
  Ok(bytes.concat([bytes.pack_le(length, 4)?, bytes.pack_le(kind, 2)?, bytes.pack_le(flags, 2)?, bytes.pack_le(sequence, 4)?, bytes.pack_le(PORT, 4)?, payload, bytes.zero((4 - length % 4) % 4)?]))
}

pure attribute(kind: Int, data: Bytes) -> Result[Bytes, Error] {
  let length = 4 + data.len()
  Ok(bytes.concat([bytes.pack_le(length, 2)?, bytes.pack_le(kind, 2)?, data, bytes.zero((4 - length % 4) % 4)?]))
}

## One data message of a dump.
export pure entry(kind: Int, sequence: Int, payload: Bytes) -> Result[Bytes, Error] {
  message(kind, NLM_F_MULTI, sequence, payload)
}

## A link message: RTM_NEWLINK with its name, hardware address and MTU.
export pure link(ifindex: Int, flags: Int, name: Str, mac: Bytes, mtu: Int) -> Result[Bytes, Error] {
  let header = bytes.concat([b"\x00\x00", bytes.pack_le(1, 2)?, bytes.pack_le(ifindex, 4)?, bytes.pack_le(flags, 4)?, bytes.zero(4)?])
  let payload = bytes.concat([header, attribute(1, mac)?, attribute(3, bytes.concat([bytes.from_text(name), b"\x00"]))?, attribute(4, bytes.pack_le(mtu, 4)?)?])
  entry(16, 1, payload)
}

## An IPv4 address message: RTM_NEWADDR with its local address.
export pure address4(ifindex: Int, prefix: Int, local: Bytes) -> Result[Bytes, Error] {
  let header = bytes.concat([b"\x02", bytes.pack_le(prefix, 1)?, b"\x00\x00", bytes.pack_le(ifindex, 4)?])
  entry(20, 2, bytes.concat([header, attribute(1, local)?, attribute(2, local)?]))
}

pure done(sequence: Int) -> Result[Bytes, Error] {
  message(NLMSG_DONE, NLM_F_MULTI, sequence, bytes.pack_le(0, 4)?)
}

## The datagrams of one complete dump: the messages in one datagram, then the
## terminator in its own.
export pure dump(sequence: Int, entries: List[Bytes]) -> Result[List[Str], Error] {
  Ok([bytes.concat(entries).base64(), done(sequence)?.base64()])
}

## The acknowledgement of a replayed address addition (RTM_NEWADDR); `errno` is the positive
## error number the kernel refused with, or 0.
export pure acknowledgement(errno: Int) -> Result[Str, Error] {
  let echoed = bytes.concat([bytes.pack_le(16, 4)?, bytes.pack_le(20, 2)?, bytes.pack_le(0, 2)?, bytes.pack_le(1, 4)?, bytes.pack_le(PORT, 4)?])
  let code = if errno == 0 { 0 } else { 4294967296 - errno }
  Ok(message(NLMSG_ERROR, 0, 1, bytes.concat([bytes.pack_le(code, 4)?, echoed]))?.base64())
}
