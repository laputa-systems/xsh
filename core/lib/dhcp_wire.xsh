##! DHCPv4 wire format (RFC 2131, 2132, 3396, 3442): message encoders and one
##! validating decoder. Nothing here touches a socket or a clock; every
##! function maps bytes and numbers to bytes, numbers or a typed rejection.
##! IPv4 addresses are unsigned 32-bit integers, so address arithmetic is
##! checked and no text is parsed on the hot path.

## Why a received datagram was refused. A reject is data: the client counts it
## and keeps running.
export error WireReject {
  TooShort
  TooLong
  BadOp(op: Int)
  BadHardware
  BadCookie
  XidMismatch
  ChaddrMismatch
  ClientIdMismatch
  OptionTruncated
  MissingEnd
  TooManyOptions
  BadOverload(value: Int)
  DuplicateOption(code: Int)
  BadOptionLength(code: Int)
  BadOptionValue(code: Int)
  MissingMessageType
  BadMessageType(kind: Int)
  MissingServerId
  MissingLeaseTime
  BadAddress(field: Str)
}

## The sender identity of a client: its hardware address and the client
## identifier (RFC 2132 option 61, hardware type 1 followed by the address).
export type Identity = {mac: Bytes, client_id: Bytes}

## The fixed header fields of a BOOTP/DHCP message. `broadcast` is the flag
## that asks the server to answer by broadcast.
export type Header = {
  op: Int,
  xid: Int,
  secs: Int,
  broadcast: Bool,
  ciaddr: Int,
  yiaddr: Int,
  siaddr: Int,
  giaddr: Int,
  chaddr: Bytes,
}

## A fully validated message. Absent options are null or empty lists; no field
## holds a value that failed its option's length or content rule.
export type Message = {
  op: Int,
  kind: Int,
  xid: Int,
  secs: Int,
  broadcast: Bool,
  ciaddr: Int,
  yiaddr: Int,
  siaddr: Int,
  giaddr: Int,
  chaddr: Bytes,
  client_id: Bytes?,
  server_id: Int?,
  requested_ip: Int?,
  lease_secs: Int?,
  renewal_secs: Int?,
  rebinding_secs: Int?,
  subnet_mask: Int?,
  routers: List[Int],
  classless_routes: List[ClasslessRoute],
  dns: List[Int],
  domain: Str?,
  broadcast_address: Int?,
  max_size: Int?,
  parameter_list: List[Int],
  text: Str?,
}

## One route of the classless static route option (RFC 3442): a destination
## network of `prefix` bits and the router that reaches it, where a router of
## 0.0.0.0 means the destination is on the link itself.
export type ClasslessRoute = {destination: Int, prefix: Int, gateway: Int}

type RawOption = {code: Int, data: Bytes, parts: Int}

type OptionRule = {code: Int, concatenates: Bool, requested: Bool}

## The DHCP message types of option 53 (RFC 2132).
export const DISCOVER = 1
## DHCPOFFER.
export const OFFER = 2
## DHCPREQUEST.
export const REQUEST = 3
## DHCPDECLINE.
export const DECLINE = 4
## DHCPACK.
export const ACK = 5
## DHCPNAK.
export const NAK = 6
## DHCPRELEASE.
export const RELEASE = 7
## DHCPINFORM.
export const INFORM = 8

## The BOOTP `op` of a client message.
export const OP_REQUEST = 1
## The BOOTP `op` of a server message.
export const OP_REPLY = 2

## The UDP port servers and relays listen on.
export const SERVER_PORT = 67
## The UDP port clients listen on.
export const CLIENT_PORT = 68
## The lease length that means the lease never expires (0xFFFFFFFF).
export const INFINITE_LEASE = 4294967295
## The shortest datagram that holds a header, the cookie and one option.
export const MIN_MESSAGE_LENGTH = 240
## The longest datagram the decoder accepts.
export const MAX_MESSAGE_LENGTH = 1500
## The most addresses kept from a router or DNS list.
export const MAX_LIST_ITEMS = 8
## The most routes kept from a classless static route option.
export const MAX_CLASSLESS_ROUTES = 32
## The most option instances one message may carry across its option areas.
export const MAX_OPTION_INSTANCES = 256
## The BOOTP minimum size encoded messages are padded to.
export const REQUEST_PAD_LENGTH = 300

const OPTION_PAD = 0
const OPTION_MASK = 1
const OPTION_ROUTERS = 3
const OPTION_DNS = 6
const OPTION_DOMAIN = 15
const OPTION_BROADCAST = 28
const OPTION_CLASSLESS = 121
const OPTION_REQUESTED_IP = 50
const OPTION_LEASE = 51
const OPTION_OVERLOAD = 52
const OPTION_TYPE = 53
const OPTION_SERVER_ID = 54
const OPTION_PARAMETERS = 55
const OPTION_MESSAGE = 56
const OPTION_MAX_SIZE = 57
const OPTION_RENEWAL = 58
const OPTION_REBINDING = 59
const OPTION_CLIENT_ID = 61
const OPTION_END = 255

const COOKIE = [99, 130, 83, 99]
const SNAME_OFFSET = 44
const FILE_OFFSET = 108
const OPTIONS_OFFSET = 240
const FILE_END = 236
const MAX_DOMAIN_LENGTH = 253
const ADVERTISED_MAX_SIZE = 1500

const OPTION_RULES: List[OptionRule] = [
  {code: OPTION_MASK, concatenates: false, requested: true},
  {code: OPTION_ROUTERS, concatenates: true, requested: true},
  {code: OPTION_DNS, concatenates: true, requested: true},
  {code: OPTION_DOMAIN, concatenates: true, requested: true},
  {code: OPTION_BROADCAST, concatenates: false, requested: true},
  {code: OPTION_REQUESTED_IP, concatenates: false, requested: false},
  {code: OPTION_LEASE, concatenates: false, requested: true},
  {code: OPTION_OVERLOAD, concatenates: false, requested: false},
  {code: OPTION_TYPE, concatenates: false, requested: false},
  {code: OPTION_SERVER_ID, concatenates: false, requested: false},
  {code: OPTION_PARAMETERS, concatenates: true, requested: false},
  {code: OPTION_MESSAGE, concatenates: true, requested: false},
  {code: OPTION_MAX_SIZE, concatenates: false, requested: false},
  {code: OPTION_RENEWAL, concatenates: false, requested: true},
  {code: OPTION_REBINDING, concatenates: false, requested: true},
  {code: OPTION_CLIENT_ID, concatenates: true, requested: false},
  {code: OPTION_CLASSLESS, concatenates: true, requested: true},
]

## The option codes this library parses and validates.
export pure implemented_options() -> List[Int] {
  [rule.code for rule in OPTION_RULES]
}

## The option codes a client asks for in option 55: exactly the implemented
## options a server sends without being told to, so a request never names an
## option this library ignores.
export pure requested_options() -> List[Int] {
  [rule.code for rule in OPTION_RULES if rule.requested]
}

pure rule_for(code: Int) -> OptionRule? {
  for rule in OPTION_RULES {
    return rule when rule.code == code
  }
  null
}

## The dotted-quad text of an address.
export pure ip_text(address: Int) -> Str {
  f"{address / 16777216 % 256}.{address / 65536 % 256}.{address / 256 % 256}.{address % 256}"
}

pure decimal_octet(text: Str) -> Int? {
  return null when text == "" or text.byte_len() > 3
  return null when text.byte_len() > 1 and text.starts_with("0")
  for digit in text {
    return null when digit not in "0123456789"
  }
  let value = text.parse_int() ?? -1
  return null when value < 0 or value > 255
  value
}

## The address of strict dotted-quad text: four decimal octets without signs,
## spaces or leading zeros. Anything else is null.
export pure ip_parse(text: Str) -> Int? {
  let parts = text.split(".")
  return null when parts.len() != 4
  var value = 0
  for part in parts {
    let octet = decimal_octet(part)
    return null when octet == null
    value = value * 256 + octet
  }
  value
}

## True for an address usable as a host, gateway or server: not 0.0.0.0/8,
## loopback, multicast or the reserved class E range with the broadcast address.
export pure is_unicast(address: Int) -> Bool {
  return false when address < 0 or address > 4294967295
  let first = address / 16777216
  first != 0 and first != 127 and first < 224
}

## The prefix length of a contiguous netmask, or null for a mask with holes.
export pure mask_prefix(mask: Int) -> Int? {
  return null when mask < 0 or mask > 4294967295
  let inverse = 4294967295 - mask
  return null unless inverse.bit_and(inverse + 1) == 0
  var remaining = inverse
  var prefix = 32
  while remaining > 0 {
    remaining = remaining / 2
    prefix -= 1
  }
  prefix
}

## The netmask of a prefix length from 0 to 32, or null.
export pure prefix_mask(prefix: Int) -> Int? {
  return null when prefix < 0 or prefix > 32
  var host = 1
  repeat 32 - prefix times {
    host *= 2
  }
  4294967295 - (host - 1)
}

## The client identity for a six-byte Ethernet address.
export pure identity(mac: Bytes) -> Result[Identity, WireReject] {
  fail .BadHardware() when mac.len() != 6
  Ok({mac, client_id: bytes.concat([b"\x01", mac])})
}

pure be32(value: Int) -> Result[Bytes, Error] {
  bytes.pack_be(value, 4)
}

## One option: code, length and value. Fails for a value over 255 bytes.
export pure option(code: Int, data: Bytes) -> Result[Bytes, Error] {
  return Err(error.failure("option value exceeds 255 bytes")) when data.len() > 255
  Ok(bytes.concat([bytes.from_ints([code, data.len()])?, data]))
}

pure option_u8(code: Int, value: Int) -> Result[Bytes, Error] {
  option(code, bytes.from_ints([value])?)
}

pure option_address(code: Int, address: Int) -> Result[Bytes, Error] {
  option(code, be32(address)?)
}

## A complete message: header, magic cookie, the given encoded options, the
## end option and zero padding up to the 300-byte BOOTP minimum.
export pure encode_message(header: Header, options: List[Bytes]) -> Result[Bytes, Error] {
  return Err(error.failure("chaddr exceeds 16 bytes")) when header.chaddr.len() > 16
  let flags = if header.broadcast { 32768 } else { 0 }
  let body = bytes.concat(
    [
      bytes.from_ints([header.op, 1, header.chaddr.len(), 0])?,
      be32(header.xid)?,
      bytes.pack_be(header.secs, 2)?,
      bytes.pack_be(flags, 2)?,
      be32(header.ciaddr)?,
      be32(header.yiaddr)?,
      be32(header.siaddr)?,
      be32(header.giaddr)?,
      header.chaddr,
      bytes.zero(16 - header.chaddr.len())?,
      bytes.zero(192)?,
      bytes.from_ints(COOKIE)?,
      bytes.concat(options),
      bytes.from_ints([OPTION_END])?,
    ],
  )
  return Ok(body) when body.len() >= REQUEST_PAD_LENGTH
  Ok(bytes.concat([body, bytes.zero(REQUEST_PAD_LENGTH - body.len())?]))
}

pure client_header(id: Identity, xid: Int, secs: Int, broadcast: Bool, ciaddr: Int) -> Header {
  {op: OP_REQUEST, xid, secs, broadcast, ciaddr, yiaddr: 0, siaddr: 0, giaddr: 0, chaddr: id.mac}
}

pure client_common(id: Identity, kind: Int) -> Result[List[Bytes], Error] {
  Ok([option_u8(OPTION_TYPE, kind)?, option(OPTION_CLIENT_ID, id.client_id)?])
}

pure client_preferences() -> Result[List[Bytes], Error] {
  Ok(
    [
      option(OPTION_MAX_SIZE, bytes.pack_be(ADVERTISED_MAX_SIZE, 2)?)?,
      option(OPTION_PARAMETERS, bytes.from_ints(requested_options())?)?,
    ],
  )
}

## DHCPDISCOVER. It asks for a broadcast answer because the interface has no
## address yet.
export pure discover(id: Identity, xid: Int, secs: Int) -> Result[Bytes, Error] {
  let options = client_common(id, DISCOVER)?.extend(client_preferences()?)
  encode_message(client_header(id, xid, secs, true, 0), options)
}

## DHCPREQUEST answering an offer (SELECTING): names the chosen address and
## the server whose offer is accepted, with `ciaddr` zero.
export pure request_selecting(id: Identity, xid: Int, secs: Int, address: Int, server: Int) -> Result[Bytes, Error] {
  let options = client_common(id, REQUEST)?
    .push(option_address(OPTION_REQUESTED_IP, address)?)
    .push(option_address(OPTION_SERVER_ID, server)?)
    .extend(client_preferences()?)
  encode_message(client_header(id, xid, secs, true, 0), options)
}

## DHCPREQUEST for a remembered address after a reboot (INIT-REBOOT): names
## the address and no server, with `ciaddr` zero.
export pure request_init_reboot(id: Identity, xid: Int, secs: Int, address: Int) -> Result[Bytes, Error] {
  let options = client_common(id, REQUEST)?
    .push(option_address(OPTION_REQUESTED_IP, address)?)
    .extend(client_preferences()?)
  encode_message(client_header(id, xid, secs, true, 0), options)
}

## DHCPREQUEST while RENEWING or REBINDING: `ciaddr` carries the bound address,
## no requested address or server is named, and the broadcast flag is clear
## because the client can receive unicast. Renewal sends it to the server and
## rebinding broadcasts the same bytes.
export pure request_bound(id: Identity, xid: Int, secs: Int, address: Int) -> Result[Bytes, Error] {
  let options = client_common(id, REQUEST)?.extend(client_preferences()?)
  encode_message(client_header(id, xid, secs, false, address), options)
}

## DHCPDECLINE of an offered address found in use.
export pure decline(id: Identity, xid: Int, address: Int, server: Int) -> Result[Bytes, Error] {
  let options = client_common(id, DECLINE)?
    .push(option_address(OPTION_REQUESTED_IP, address)?)
    .push(option_address(OPTION_SERVER_ID, server)?)
  encode_message(client_header(id, xid, 0, false, 0), options)
}

## DHCPRELEASE of the bound address, unicast to the server that granted it.
export pure release(id: Identity, xid: Int, address: Int, server: Int) -> Result[Bytes, Error] {
  let options = client_common(id, RELEASE)?.push(option_address(OPTION_SERVER_ID, server)?)
  encode_message(client_header(id, xid, 0, false, address), options)
}

pure walk_area(data: Bytes, start: Int, stop: Int) -> Result[List[RawOption], WireReject] {
  var found: List[RawOption] = []
  var position = start
  while position < stop {
    let code = data.byte_at(position) ?? 0
    if code == OPTION_PAD {
      position += 1
      continue
    }
    return Ok(found) when code == OPTION_END
    fail .OptionTruncated() when position + 1 >= stop
    let length = data.byte_at(position + 1) ?? 0
    let value_start = position + 2
    fail .OptionTruncated() when value_start + length > stop
    fail .TooManyOptions() when found.len() >= MAX_OPTION_INSTANCES
    found += [{code, data: data[value_start..value_start + length], parts: 1}]
    position = value_start + length
  }
  Err(.MissingEnd())
}

pure merge_options(instances: List[RawOption]) -> Result[List[RawOption], WireReject] {
  var merged: List[RawOption] = []
  for instance in instances {
    guard let rule = rule_for(instance.code) else { continue }
    var index = -1
    for position in range(merged.len()) {
      if merged[position].code == instance.code { index = position }
    }
    if index < 0 {
      merged += [instance]
      continue
    }
    fail .DuplicateOption(instance.code) unless rule.concatenates
    let existing = merged[index]
    merged[index] = {code: existing.code, data: bytes.concat([existing.data, instance.data]), parts: existing.parts + 1}
  }
  Ok(merged)
}

pure find_option(options: List[RawOption], code: Int) -> Bytes? {
  for candidate in options {
    return candidate.data when candidate.code == code
  }
  null
}

pure read_u32(data: Bytes, offset: Int) -> Int {
  bytes.unpack_be(data, 4, offset) ?? 0
}

pure single_byte(options: List[RawOption], code: Int) -> Result[Int?, WireReject] {
  guard let data = find_option(options, code) else { return Ok(null) }
  fail .BadOptionLength(code) when data.len() != 1
  Ok(data.byte_at(0))
}

pure single_word(options: List[RawOption], code: Int) -> Result[Int?, WireReject] {
  guard let data = find_option(options, code) else { return Ok(null) }
  fail .BadOptionLength(code) when data.len() != 4
  Ok(read_u32(data, 0))
}

pure single_short(options: List[RawOption], code: Int) -> Result[Int?, WireReject] {
  guard let data = find_option(options, code) else { return Ok(null) }
  fail .BadOptionLength(code) when data.len() != 2
  Ok(bytes.unpack_be(data, 2, 0) ?? 0)
}

pure single_address(options: List[RawOption], code: Int, field: Str) -> Result[Int?, WireReject] {
  guard let value = single_word(options, code)? else { return Ok(null) }
  fail .BadAddress(field) unless is_unicast(value)
  Ok(value)
}

pure address_list(options: List[RawOption], code: Int, field: Str) -> Result[List[Int], WireReject] {
  guard let data = find_option(options, code) else { return Ok([]) }
  fail .BadOptionLength(code) when data.is_empty() or data.len() % 4 != 0
  var addresses = []
  for index in range(data.len() / 4) {
    let address = read_u32(data, index * 4)
    fail .BadAddress(field) unless is_unicast(address)
    continue when address in addresses
    addresses += [address]
  }
  Ok(addresses |> take(MAX_LIST_ITEMS))
}

pure routable_destination(destination: Int, prefix: Int) -> Bool {
  return true when prefix == 0
  let first = destination / 16777216
  return false when first >= 224 or first == 127
  first != 0 or prefix < 8
}

pure classless_routes(options: List[RawOption]) -> Result[List[ClasslessRoute], WireReject] {
  guard let data = find_option(options, OPTION_CLASSLESS) else { return Ok([]) }
  fail .BadOptionLength(OPTION_CLASSLESS) when data.len() < 5
  var routes: List[ClasslessRoute] = []
  var offset = 0
  while offset < data.len() {
    let prefix = data.byte_at(offset) ?? 0
    fail .BadOptionValue(OPTION_CLASSLESS) when prefix > 32
    let significant = (prefix + 7) / 8
    fail .BadOptionLength(OPTION_CLASSLESS) when offset + 1 + significant + 4 > data.len()
    var destination = 0
    for index in range(4) {
      let octet = if index < significant { data.byte_at(offset + 1 + index) ?? 0 } else { 0 }
      destination = destination * 256 + octet
    }
    let gateway = read_u32(data, offset + 1 + significant)
    fail .BadOptionValue(OPTION_CLASSLESS) unless destination.bit_and(prefix_mask(prefix) ?? 0) == destination
    fail .BadOptionValue(OPTION_CLASSLESS) unless routable_destination(destination, prefix)
    fail .BadAddress("classless_gateway") unless gateway == 0 or is_unicast(gateway)
    for held in routes {
      fail .BadOptionValue(OPTION_CLASSLESS) when held.destination == destination and held.prefix == prefix
    }
    fail .BadOptionValue(OPTION_CLASSLESS) when routes.len() >= MAX_CLASSLESS_ROUTES
    routes += [ClasslessRoute(destination:, prefix:, gateway:)]
    offset += 1 + significant + 4
  }
  Ok(routes)
}

pure is_label(label: Str) -> Bool {
  return false when label == "" or label.byte_len() > 63
  return false when label.starts_with("-") or label.ends_with("-")
  for character in label {
    return false when character not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
  }
  true
}

## True for a name made of one to 127 dot-separated labels of letters, digits,
## hyphens and underscores, at most 253 bytes in all. A trailing dot is not
## accepted. This is the only form a lease or file may place in a resolver
## search list.
export pure is_domain_name(text: Str) -> Bool {
  return false when text == "" or text.byte_len() > MAX_DOMAIN_LENGTH
  for label in text.split(".") {
    return false unless is_label(label)
  }
  true
}

pure strip_trailing_nuls(data: Bytes) -> Bytes {
  var end = data.len()
  while end > 0 and data.byte_at(end - 1) == 0 {
    end -= 1
  }
  data[..end]
}

pure domain_name(options: List[RawOption]) -> Result[Str?, WireReject] {
  guard let raw = find_option(options, OPTION_DOMAIN) else { return Ok(null) }
  let data = strip_trailing_nuls(raw)
  fail .BadOptionLength(OPTION_DOMAIN) when data.is_empty() or data.len() > MAX_DOMAIN_LENGTH
  let decoded = data.utf8()
  guard decoded is Ok(_) else { fail .BadOptionValue(OPTION_DOMAIN) }
  let text = decoded ?? ""
  let trimmed = if text.ends_with(".") { text.byte_slice(0, length: text.byte_len() - 1) } else { text }
  fail .BadOptionValue(OPTION_DOMAIN) unless is_domain_name(trimmed)
  Ok(trimmed)
}

pure printable_text(options: List[RawOption], code: Int) -> Result[Str?, WireReject] {
  guard let raw = find_option(options, code) else { return Ok(null) }
  let data = strip_trailing_nuls(raw)
  fail .BadOptionLength(code) when data.is_empty()
  let values = [if value >= 32 and value < 127 { value } else { 63 } for value in bytes_to_ints(data)]
  Ok((bytes.from_ints(values) ?? b"").utf8() ?? "")
}

pure bytes_to_ints(data: Bytes) -> List[Int] {
  [data.byte_at(index) ?? 0 for index in range(data.len())]
}

pure parameter_codes(options: List[RawOption]) -> Result[List[Int], WireReject] {
  guard let data = find_option(options, OPTION_PARAMETERS) else { return Ok([]) }
  fail .BadOptionLength(OPTION_PARAMETERS) when data.is_empty()
  Ok(bytes_to_ints(data))
}

pure client_identifier(options: List[RawOption]) -> Result[Bytes?, WireReject] {
  guard let data = find_option(options, OPTION_CLIENT_ID) else { return Ok(null) }
  fail .BadOptionLength(OPTION_CLIENT_ID) when data.len() < 2 or data.len() > 255
  Ok(data)
}

pure subnet_mask(options: List[RawOption]) -> Result[Int?, WireReject] {
  guard let mask = single_word(options, OPTION_MASK)? else { return Ok(null) }
  fail .BadOptionValue(OPTION_MASK) when mask_prefix(mask) == null
  Ok(mask)
}

pure message_kind(options: List[RawOption]) -> Result[Int, WireReject] {
  guard let kind = single_byte(options, OPTION_TYPE)? else { fail .MissingMessageType() }
  fail .BadMessageType(kind) when kind < DISCOVER or kind > INFORM
  Ok(kind)
}

pure overload_areas(options: List[RawOption]) -> Result[Int, WireReject] {
  guard let value = single_byte(options, OPTION_OVERLOAD)? else { return Ok(0) }
  fail .BadOverload(value) when value < 1 or value > 3
  Ok(value)
}

pure extra_area_options(data: Bytes, overload: Int) -> Result[List[RawOption], WireReject] {
  let file_area = if overload == 1 or overload == 3 { walk_area(data, FILE_OFFSET, FILE_END)? } else { [] }
  let sname_area = if overload == 2 or overload == 3 { walk_area(data, SNAME_OFFSET, FILE_OFFSET)? } else { [] }
  let extra = file_area.extend(sname_area)
  for instance in extra {
    fail .BadOverload(instance.code) when instance.code == OPTION_OVERLOAD
  }
  Ok(extra)
}

## A stable name for a rejection, suitable as a counter key.
export pure reject_reason(reject: WireReject) -> Str {
  match reject {
    WireReject.TooShort => "too_short"
    WireReject.TooLong => "too_long"
    WireReject.BadOp {..} => "bad_op"
    WireReject.BadHardware => "bad_hardware"
    WireReject.BadCookie => "bad_cookie"
    WireReject.XidMismatch => "xid_mismatch"
    WireReject.ChaddrMismatch => "chaddr_mismatch"
    WireReject.ClientIdMismatch => "client_id_mismatch"
    WireReject.OptionTruncated => "option_truncated"
    WireReject.MissingEnd => "missing_end"
    WireReject.TooManyOptions => "too_many_options"
    WireReject.BadOverload {..} => "bad_overload"
    WireReject.DuplicateOption {..} => "duplicate_option"
    WireReject.BadOptionLength {..} => "bad_option_length"
    WireReject.BadOptionValue {..} => "bad_option_value"
    WireReject.MissingMessageType => "missing_message_type"
    WireReject.BadMessageType {..} => "bad_message_type"
    WireReject.MissingServerId => "missing_server_id"
    WireReject.MissingLeaseTime => "missing_lease_time"
    WireReject.BadAddress {..} => "bad_address"
  }
}

## Validates and decodes a datagram. It accepts both directions; use
## `accept_reply` to also check a reply against the client's request.
## The checks cover the length bounds, hardware type and length, the magic
## cookie, every option's framing and length, option overload and RFC 3396
## concatenation, duplicate singletons, the message type, and the address
## classes of every address the client could act on.
export pure decode(data: Bytes) -> Result[Message, WireReject] {
  fail .TooShort() when data.len() < MIN_MESSAGE_LENGTH
  fail .TooLong() when data.len() > MAX_MESSAGE_LENGTH
  let op = data.byte_at(0) ?? 0
  fail .BadOp(op) when op != OP_REQUEST and op != OP_REPLY
  let hardware_length = data.byte_at(2) ?? 0
  fail .BadHardware() when data.byte_at(1) != 1 or hardware_length != 6
  fail .BadCookie() when bytes_to_ints(data[FILE_END..OPTIONS_OFFSET]) != COOKIE
  let primary = walk_area(data, OPTIONS_OFFSET, data.len())?
  let overload = overload_areas(merge_options(primary)?)?
  let options = merge_options(primary.extend(extra_area_options(data, overload)?))?
  let kind = message_kind(options)?
  let flags = bytes.unpack_be(data, 2, 10) ?? 0
  let chaddr = data[28..28 + hardware_length]
  let message = Message(
    op:,
    kind:,
    xid: read_u32(data, 4),
    secs: bytes.unpack_be(data, 2, 8) ?? 0,
    broadcast: flags >= 32768,
    ciaddr: read_u32(data, 12),
    yiaddr: read_u32(data, 16),
    siaddr: read_u32(data, 20),
    giaddr: read_u32(data, 24),
    chaddr:,
    client_id: client_identifier(options)?,
    server_id: single_address(options, OPTION_SERVER_ID, "server_id")?,
    requested_ip: single_address(options, OPTION_REQUESTED_IP, "requested_ip")?,
    lease_secs: single_word(options, OPTION_LEASE)?,
    renewal_secs: single_word(options, OPTION_RENEWAL)?,
    rebinding_secs: single_word(options, OPTION_REBINDING)?,
    subnet_mask: subnet_mask(options)?,
    routers: address_list(options, OPTION_ROUTERS, "router")?,
    classless_routes: classless_routes(options)?,
    dns: address_list(options, OPTION_DNS, "dns")?,
    domain: domain_name(options)?,
    broadcast_address: single_word(options, OPTION_BROADCAST)?,
    max_size: single_short(options, OPTION_MAX_SIZE)?,
    parameter_list: parameter_codes(options)?,
    text: printable_text(options, OPTION_MESSAGE)?,
  )
  Ok(message)
}

## Decodes a datagram that must answer this client's request `xid`: a
## BOOTREPLY with the same transaction id, this client's hardware address and,
## when the server echoes it, this client identifier. OFFER and ACK also need a
## server identifier, a usable assigned address and a lease time; NAK needs
## the server identifier.
export pure accept_reply(data: Bytes, id: Identity, xid: Int) -> Result[Message, WireReject] {
  let message = decode(data)?
  fail .BadOp(message.op) when message.op != OP_REPLY
  fail .XidMismatch() when message.xid != xid
  fail .ChaddrMismatch() when message.chaddr != id.mac
  if let echoed = message.client_id {
    fail .ClientIdMismatch() when echoed != id.client_id
  }
  if message.kind == OFFER or message.kind == ACK {
    fail .MissingServerId() when message.server_id == null
    fail .BadAddress("yiaddr") unless is_unicast(message.yiaddr)
    fail .MissingLeaseTime() when message.lease_secs == null
  } else if message.kind == NAK {
    fail .MissingServerId() when message.server_id == null
  }
  Ok(message)
}
