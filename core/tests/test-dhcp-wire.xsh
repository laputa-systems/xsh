use core.lib.dhcp_wire as wire

const MAC = b"\x02\x00\x00\x00\x00\x01"
const OTHER_MAC = b"\x02\x00\x00\x00\x00\x02"
const SERVER = 3232235777
const OFFERED = 3232235876
const XID = 305419896

pure client() -> wire.Identity {
  if let Ok(id) = wire.identity(MAC) {
    id
  } else {
    {mac: b"", client_id: b""}
  }
}

pure be32(value: Int) -> Bytes {
  bytes.pack_be(value, 4) ?? b""
}

pure ints(values: List[Int]) -> Bytes {
  bytes.from_ints(values) ?? b""
}

pure option(code: Int, data: Bytes) -> Bytes {
  wire.option(code, data) ?? b""
}

pure reply_header(xid: Int, yiaddr: Int) -> wire.Header {
  {op: 2, xid, secs: 0, broadcast: false, ciaddr: 0, yiaddr, siaddr: 0, giaddr: 0, chaddr: MAC}
}

pure base_options(kind: Int) -> List[Bytes] {
  [option(53, ints([kind])), option(54, be32(SERVER)), option(51, be32(3600))]
}

pure message(header: wire.Header, options: List[Bytes]) -> Bytes {
  wire.encode_message(header, options) ?? b""
}

pure offer_with(extra: List[Bytes]) -> Bytes {
  message(reply_header(XID, OFFERED), base_options(2).extend(extra))
}

pure offer() -> Bytes {
  offer_with([])
}

pure patched(data: Bytes, offset: Int, replacement: Bytes) -> Bytes {
  bytes.concat([data[..offset], replacement, data[offset + replacement.len()..]])
}

pure with_raw_options(raw: Bytes) -> Bytes {
  bytes.concat([offer()[..240], raw])
}

pure verdict(data: Bytes) -> Str {
  match wire.accept_reply(data, client(), XID) {
    Ok(_) => "accepted"
    Err(wire.WireReject.DuplicateOption {code}) => f"duplicate_{code}"
    Err(wire.WireReject.BadOptionLength {code}) => f"length_{code}"
    Err(wire.WireReject.BadOptionValue {code}) => f"value_{code}"
    Err(wire.WireReject.BadAddress {field}) => f"address_{field}"
    Err(reject) => wire.reject_reason(reject)
  }
}

proc decoded(data: Bytes) -> Result[wire.Message] {
  match wire.accept_reply(data, client(), XID) {
    Ok(value) => value
    Err(reject) => Err(error.failure(f"rejected: {wire.reject_reason(reject)}"))
  }
}

test test_discover_has_the_exact_wire_bytes {
  let datagram = wire.discover(client(), XID, 0)?
  let header = bytes.concat([b"\x01\x01\x06\x00", b"\x12\x34\x56\x78", b"\x00\x00", b"\x80\x00", bytes.zero(16)?, MAC, bytes.zero(10)?])
  assert datagram.len() == 300
  assert datagram[..44] == header
  assert datagram[44..236] == bytes.zero(192)?
  assert datagram[236..240] == b"\x63\x82\x53\x63"
  let options = bytes.concat([b"\x35\x01\x01", b"\x3d\x07\x01", MAC, b"\x39\x02\x05\xdc", b"\x37\x08\x01\x03\x06\x0f\x1c\x33\x3a\x3b", b"\xff"])
  assert datagram[240..240 + options.len()] == options
  assert datagram[240 + options.len()..] == bytes.zero(300 - 240 - options.len())?
}

test test_requested_options_are_a_subset_of_the_implemented_options {
  let implemented = wire.implemented_options()
  for code in wire.requested_options() {
    assert code in implemented
  }
  assert wire.requested_options() == [1, 3, 6, 15, 28, 51, 58, 59]
  assert 121 not in implemented
}

test test_request_forms_set_ciaddr_requested_address_server_and_flag {
  let selecting = wire.decode(wire.request_selecting(client(), 7, 2, OFFERED, SERVER)?)?
  assert selecting.kind == 3 and selecting.op == 1
  assert selecting.ciaddr == 0 and selecting.siaddr == 0 and selecting.yiaddr == 0
  assert selecting.requested_ip == OFFERED and selecting.server_id == SERVER
  assert selecting.broadcast and selecting.secs == 2 and selecting.xid == 7
  assert selecting.parameter_list == wire.requested_options()
  assert selecting.client_id == client().client_id and selecting.chaddr == MAC

  let reboot = wire.decode(wire.request_init_reboot(client(), 8, 0, OFFERED)?)?
  assert reboot.kind == 3 and reboot.ciaddr == 0 and reboot.broadcast
  assert reboot.requested_ip == OFFERED and reboot.server_id == null

  let bound = wire.decode(wire.request_bound(client(), 9, 4, OFFERED)?)?
  assert bound.kind == 3 and bound.ciaddr == OFFERED and ! bound.broadcast
  assert bound.requested_ip == null and bound.server_id == null

  let decline = wire.decode(wire.decline(client(), 10, OFFERED, SERVER)?)?
  assert decline.kind == 4 and decline.ciaddr == 0 and ! decline.broadcast
  assert decline.requested_ip == OFFERED and decline.server_id == SERVER

  let release = wire.decode(wire.release(client(), 11, OFFERED, SERVER)?)?
  assert release.kind == 7 and release.ciaddr == OFFERED and ! release.broadcast
  assert release.server_id == SERVER and release.requested_ip == null
}

test test_request_bytes_match_the_rfc_2131_field_table {
  let datagram = wire.request_bound(client(), XID, 0, OFFERED)?
  assert datagram[12..16] == be32(OFFERED)
  assert datagram[10..12] == b"\x00\x00"
  let released = wire.release(client(), XID, OFFERED, SERVER)?
  assert released[12..16] == be32(OFFERED)
  assert released[240..243] == b"\x35\x01\x07"
  let declined = wire.decline(client(), XID, OFFERED, SERVER)?
  assert declined[12..16] == bytes.zero(4)?
  assert declined[240..243] == b"\x35\x01\x04"
}

test test_a_valid_offer_is_accepted_with_every_field {
  let routers = option(3, bytes.concat([be32(SERVER), be32(SERVER + 1)]))
  let servers = be32(167772161) |> option(6, _)
  let datagram = offer_with([option(1, b"\xff\xff\xff\x00"), routers, servers, option(15, b"example.org"), option(28, be32(3232236031)), option(58, be32(1800)), option(59, be32(3150)), option(56, b"hello")])
  let value = decoded(datagram)?
  assert value.kind == 2 and value.yiaddr == OFFERED and value.server_id == SERVER
  assert value.lease_secs == 3600 and value.renewal_secs == 1800 and value.rebinding_secs == 3150
  assert value.subnet_mask == 4294967040 and wire.mask_prefix(value.subnet_mask ?? 0) == 24
  assert value.routers == [SERVER, SERVER + 1] and value.dns == [167772161]
  assert value.domain == "example.org" and value.broadcast_address == 3232236031
  assert value.text == "hello"
}

test test_reply_checks_name_the_first_failing_field {
  let good = offer()
  assert verdict(good) == "accepted"
  assert verdict(good[..239]) == "too_short"
  assert verdict(b"") == "too_short"
  assert verdict(bytes.concat([good, bytes.zero(1300)?])) == "too_long"
  assert verdict(patched(good, 0, b"\x01")) == "bad_op"
  assert verdict(patched(good, 0, b"\x03")) == "bad_op"
  assert verdict(patched(good, 1, b"\x06")) == "bad_hardware"
  assert verdict(patched(good, 2, b"\x08")) == "bad_hardware"
  assert verdict(patched(good, 236, b"\x63\x82\x53\x64")) == "bad_cookie"
  assert verdict(patched(good, 4, b"\x00\x00\x00\x01")) == "xid_mismatch"
  assert verdict(patched(good, 28, OTHER_MAC)) == "chaddr_mismatch"
  let foreign_id = offer_with([option(61, bytes.concat([b"\x01", OTHER_MAC]))])
  assert verdict(foreign_id) == "client_id_mismatch"
  let echoed_id = offer_with([option(61, client().client_id)])
  assert verdict(echoed_id) == "accepted"
}

test test_option_framing_errors_are_typed_not_raised {
  assert verdict(with_raw_options(b"\x06\x08\x01\x02")) == "option_truncated"
  assert verdict(with_raw_options(b"\x06")) == "option_truncated"
  assert verdict(with_raw_options(b"\x35\x01\x02\x36\x04\xc0\xa8\x01\x01\x33\x04\x00\x00\x0e\x10")) == "missing_end"
  assert verdict(with_raw_options(b"\x35\x01\x02\x00\x00\x00")) == "missing_end"
  assert verdict(with_raw_options(bytes.concat([b"\x35\x01\x02", bytes.zero(0)?]))) == "missing_end"
}

test test_missing_and_bad_message_type_and_server_id {
  let no_type = message(reply_header(XID, OFFERED), [option(54, be32(SERVER)), option(51, be32(3600))])
  assert verdict(no_type) == "missing_message_type"
  for kind in [0, 9, 255] {
    let odd = message(reply_header(XID, OFFERED), [option(53, ints([kind])), option(54, be32(SERVER)), option(51, be32(3600))])
    assert verdict(odd) == "bad_message_type"
  }
  let no_server = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(51, be32(3600))])
  assert verdict(no_server) == "missing_server_id"
  let no_lease = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(54, be32(SERVER))])
  assert verdict(no_lease) == "missing_lease_time"
  let nak = message(reply_header(XID, 0), [option(53, ints([6])), option(54, be32(SERVER))])
  assert verdict(nak) == "accepted"
  let anonymous_nak = message(reply_header(XID, 0), [option(53, ints([6]))])
  assert verdict(anonymous_nak) == "missing_server_id"
}

test test_option_length_and_value_rules {
  assert verdict(offer_with([option(51, b"\x00\x00\x0e")])) == "duplicate_51"
  let short_lease = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(54, be32(SERVER)), option(51, b"\x00\x00\x0e")])
  assert verdict(short_lease) == "length_51"
  let wide_type = message(reply_header(XID, OFFERED), [option(53, ints([2, 2])), option(54, be32(SERVER)), option(51, be32(3600))])
  assert verdict(wide_type) == "length_53"
  assert verdict(offer_with([option(3, b"\x01\x02\x03\x04\x05")])) == "length_3"
  assert verdict(offer_with([option(3, b"")])) == "length_3"
  assert verdict(offer_with([option(6, b"\x08\x08\x08")])) == "length_6"
  assert verdict(offer_with([option(1, b"\xff\xff")])) == "length_1"
  assert verdict(offer_with([option(1, b"\xff\x00\xff\x00")])) == "value_1"
  assert verdict(offer_with([option(1, b"\xff\xff\xff\xff")])) == "accepted"
  assert verdict(offer_with([option(1, b"\x00\x00\x00\x00")])) == "accepted"
  assert verdict(offer_with([option(15, b"bad domain")])) == "value_15"
  assert verdict(offer_with([option(15, b"a..b")])) == "value_15"
  assert verdict(offer_with([option(15, b"-a.b")])) == "value_15"
  assert verdict(offer_with([option(15, b"a\nnameserver 1.1.1.1")])) == "value_15"
  assert verdict(offer_with([option(15, b"\xff\xfe")])) == "value_15"
  assert verdict(offer_with([option(15, b"")])) == "length_15"
  assert verdict(offer_with([option(15, b"example.org\x00")])) == "accepted"
  assert verdict(offer_with([option(57, b"\x05")])) == "length_57"
  assert verdict(offer_with([option(61, b"\x01")])) == "length_61"
  assert verdict(offer_with([option(55, b"")])) == "length_55"
  assert verdict(offer_with([option(56, b"")])) == "length_56"
  let text = decoded(offer_with([option(56, b"no\x01lease\x7f")]))?
  assert text.text == "no?lease?"
}

test test_addresses_must_be_usable_unicast {
  for bad in [0, 2130706433, 3758096385, 4026531841, 4294967295] {
    let yiaddr_offer = message(reply_header(XID, bad), base_options(2))
    assert verdict(yiaddr_offer) == "address_yiaddr"
    let server_offer = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(54, be32(bad)), option(51, be32(3600))])
    assert verdict(server_offer) == "address_server_id"
    assert verdict(offer_with([option(3, be32(bad))])) == "address_router"
    assert verdict(offer_with([option(6, be32(bad))])) == "address_dns"
  }
}

test test_duplicate_singletons_reject_and_lists_concatenate {
  assert verdict(offer_with([option(54, be32(SERVER))])) == "duplicate_54"
  assert verdict(offer_with([option(53, ints([2]))])) == "duplicate_53"
  let split_dns = decoded(offer_with([option(6, be32(167772161)), option(6, be32(167772162))]))?
  assert split_dns.dns == [167772161, 167772162]
  let split_domain = decoded(offer_with([option(15, b"exam"), option(15, b"ple.org")]))?
  assert split_domain.domain == "example.org"
  let long_dns = [be32(167772160 + n) for n in range(1, 11)]
  let bounded = decoded(offer_with([option(6, bytes.concat(long_dns))]))?
  assert bounded.dns == [167772160 + n for n in range(1, 9)]
  let repeated = decoded(offer_with([option(6, bytes.concat([be32(167772161), be32(167772161)]))]))?
  assert repeated.dns == [167772161]
}

test test_option_overload_reads_file_and_sname_areas {
  let base = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(54, be32(SERVER)), option(51, be32(3600)), option(52, ints([3]))])
  let file_options = bytes.concat([option(6, be32(167772161)), b"\xff"])
  let sname_options = bytes.concat([option(15, b"example.org"), b"\xff"])
  let two_areas = patched(base, 108, file_options) |> patched(_, 44, sname_options)
  let value = decoded(two_areas)?
  assert value.dns == [167772161] and value.domain == "example.org"

  let file_only = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(54, be32(SERVER)), option(51, be32(3600)), option(52, ints([1]))])
  let only = decoded(patched(file_only, 108, file_options))?
  assert only.dns == [167772161] and only.domain == null
  assert verdict(patched(file_only, 44, sname_options)) == "missing_end"

  assert verdict(patched(base, 108, b"\x06\x04\x0a")) == "missing_end"
  let bad_value = message(reply_header(XID, OFFERED), [option(53, ints([2])), option(54, be32(SERVER)), option(51, be32(3600)), option(52, ints([4]))])
  assert verdict(bad_value) == "bad_overload"
  let nested = patched(base, 108, bytes.concat([option(52, ints([1])), b"\xff"]))
  assert verdict(patched(nested, 44, b"\xff")) == "bad_overload"
}

test test_instance_count_is_bounded {
  let flood = bytes.concat([bytes.concat([b"\xc8\x00" for _ in range(300)]), b"\xff"])
  let head = offer()[..240]
  assert verdict(bytes.concat([head, option(53, ints([2])), flood])) == "too_many_options"
}

test test_every_truncation_of_a_reply_is_a_typed_outcome {
  let complete = offer_with([option(6, be32(167772161)), option(15, b"example.org")])
  let end_offset = 240 + 3 + 6 + 6 + 6 + 13 + 1
  for length in range(complete.len() + 1) {
    let outcome = verdict(complete[..length])
    if length < end_offset {
      assert outcome != "accepted", f"{length}: {outcome}"
    } else {
      assert outcome == "accepted", f"{length}: {outcome}"
    }
  }
}

pure next_random(state: Int) -> Int {
  (state * 1103515245 + 12345) % 2147483648
}

test test_mutated_replies_yield_only_ok_or_typed_rejects {
  let seed = offer_with([option(1, b"\xff\xff\xff\x00"), option(3, be32(SERVER)), option(6, be32(167772161)), option(15, b"example.org"), option(58, be32(1800)), option(59, be32(3150))])
  var state = 20260101
  var accepted = 0
  var reasons = []
  repeat 1500 times {
    var mutant = seed
    state = next_random(state)
    let edits = 1 + state / 65536 % 4
    repeat edits times {
      state = next_random(state)
      let offset = state / 256 % 300
      state = next_random(state)
      mutant = patched(mutant, offset, ints([state / 256 % 256]))
    }
    match wire.accept_reply(mutant, client(), XID) {
      Ok(value) => {
        accepted += 1
        assert wire.is_unicast(value.yiaddr)
        assert value.routers.len() <= wire.MAX_LIST_ITEMS and value.dns.len() <= wire.MAX_LIST_ITEMS
        for address in value.routers.extend(value.dns) {
          assert wire.is_unicast(address)
        }
        if let mask = value.subnet_mask {
          assert wire.mask_prefix(mask) != null
        }
      }
      Err(reject) => {
        let reason = wire.reject_reason(reject)
        if reason not in reasons { reasons += [reason] }
      }
    }
  }
  assert accepted > 0
  assert reasons.len() >= 5, reasons.join(",")
}

test test_address_text_masks_and_classes {
  assert wire.ip_text(3232235876) == "192.168.1.100"
  assert wire.ip_text(0) == "0.0.0.0"
  assert wire.ip_text(4294967295) == "255.255.255.255"
  assert wire.ip_parse("192.168.1.100") == 3232235876
  for text in ["", "1.2.3", "1.2.3.4.5", "256.1.1.1", "01.2.3.4", "1.2.3.-4", "1.2.3.+4", "a.b.c.d", "1..3.4", " 1.2.3.4", "1.2.3.4 "] {
    assert wire.ip_parse(text) == null, text
  }
  assert wire.mask_prefix(4294967295) == 32
  assert wire.mask_prefix(0) == 0
  assert wire.mask_prefix(4294967040) == 24
  assert wire.mask_prefix(4278255360) == null
  assert wire.mask_prefix(-1) == null
  assert wire.mask_prefix(4294967296) == null
  for prefix in range(33) {
    assert wire.mask_prefix(wire.prefix_mask(prefix) ?? -1) == prefix
  }
  assert wire.prefix_mask(33) == null
  assert wire.is_unicast(wire.ip_parse("10.1.2.3") ?? 0)
  for text in ["0.1.2.3", "127.0.0.1", "224.0.0.1", "239.255.255.255", "240.0.0.1", "255.255.255.255"] {
    assert ! wire.is_unicast(wire.ip_parse(text) ?? 0), text
  }
}

test test_identity_needs_an_ethernet_address {
  assert wire.identity(b"\x01\x02\x03") is Err(_)
  assert wire.identity(MAC)?.client_id == bytes.concat([b"\x01", MAC])
}

test test_option_encoder_refuses_oversized_values {
  assert wire.option(7, bytes.zero(256)?) is Err(_)
  assert wire.option(7, bytes.zero(255)?)?.len() == 257
}
