use core.lib.dhcp_frame as frame_codec
use core.lib.dhcp_wire as wire
use core.tests.support.dhcp_pcap as pcap

const MAC = b"\x02\x00\x00\x00\x00\x01"
const OTHER = b"\x02\x00\x00\x00\x00\x09"
const BROADCAST = b"\xff\xff\xff\xff\xff\xff"
const SERVER = 3232248321

pure payload() -> Bytes {
  let header = {op: 2, xid: 7, secs: 0, broadcast: true, ciaddr: 0, yiaddr: 3232248420, siaddr: 0, giaddr: 0, chaddr: MAC}
  wire.encode_message(header, [wire.option(53, b"\x02") ?? b"", wire.option(54, b"\xc0\xa8\x32\x01") ?? b"", wire.option(51, b"\x00\x00\x02\x58") ?? b""]) ?? b""
}

pure good() -> Bytes {
  pcap.udp_frame(BROADCAST, b"\x02\x00\x00\x00\x00\x02", SERVER, 4294967295, 67, 68, payload())
}

pure verdict(frame: Bytes) -> Str {
  match frame_codec.server_datagram(frame, MAC) {
    Ok(_) => "accepted"
    Err(reject) => frame_codec.reject_reason(reject)
  }
}

pure patched(data: Bytes, offset: Int, replacement: Bytes) -> Bytes {
  bytes.concat([data[..offset], replacement, data[offset + replacement.len()..]])
}

test test_a_server_frame_yields_its_payload_and_addresses {
  let found = frame_codec.server_datagram(good(), MAC)?
  assert found.payload == payload() and found.source == SERVER and found.destination == 4294967295
  assert found.source_port == 67 and found.destination_port == 68
  let unicast = pcap.udp_frame(MAC, b"\x02\x00\x00\x00\x00\x02", SERVER, 3232248420, 67, 68, payload())
  assert verdict(unicast) == "accepted"
}

test test_the_independent_dissector_reads_the_same_frame {
  let dissected = pcap.dissect(good())
  assert dissected.ip?.source == SERVER and dissected.ip?.checksum_ok == true
  assert dissected.udp?.payload == payload()
}

test test_every_header_fault_is_a_typed_rejection {
  let frame = good()
  assert verdict(patched(frame, 0, OTHER)) == "frame_wrong_hardware"
  assert verdict(patched(frame, 12, b"\x86\xdd")) == "frame_not_ipv4"
  assert verdict(patched(frame, 14, b"\x65")) == "frame_bad_header"
  assert verdict(patched(frame, 14, b"\x44")) == "frame_bad_header"
  assert verdict(patched(frame, 24, b"\x00\x00")) == "frame_bad_checksum"
  assert verdict(patched(frame, 22, b"\x3f")) == "frame_bad_checksum"
  assert verdict(patched(patched(frame, 20, b"\x20\x00"), 24, b"\x00\x00")) == "frame_bad_checksum"
  assert verdict(patched(frame, 16, b"\x0f\xff")) == "frame_bad_checksum"
  assert verdict(patched(frame, 34, b"\x00\x44")) == "frame_wrong_port"
  assert verdict(patched(frame, 36, b"\x00\x43")) == "frame_wrong_port"
  assert verdict(patched(frame, 38, b"\x00\x04")) == "frame_bad_length"
  assert verdict(patched(frame, 38, b"\x3f\xff")) == "frame_bad_length"
  assert verdict(b"") == "frame_too_short"
  assert verdict(good()[..40]) == "frame_too_short"
}

pure with_valid_checksum(frame: Bytes, offset: Int, replacement: Bytes) -> Bytes {
  let zeroed = patched(frame, offset, replacement) |> patched(_, 24, b"\x00\x00")
  patched(zeroed, 24, bytes.pack_be(pcap.header_checksum(zeroed[14..34]), 2) ?? b"")
}

test test_fragments_other_protocols_and_inconsistent_lengths_are_refused_with_valid_checksums {
  let frame = good()
  assert verdict(with_valid_checksum(frame, 20, b"\x20\x00")) == "frame_fragment"
  assert verdict(with_valid_checksum(frame, 20, b"\x00\x08")) == "frame_fragment"
  assert verdict(with_valid_checksum(frame, 23, b"\x06")) == "frame_not_udp"
  assert verdict(with_valid_checksum(frame, 16, b"\x0f\xff")) == "frame_bad_length"
  assert verdict(with_valid_checksum(frame, 16, b"\x00\x1b")) == "frame_bad_length"
}

test test_every_truncation_of_a_frame_is_a_typed_outcome {
  let frame = good()
  for length in range(frame.len()) {
    assert verdict(frame[..length]) != "accepted", f"{length}"
  }
  assert verdict(frame) == "accepted"
  assert verdict(bytes.concat([frame, b"\x00\x00\x00\x00"])) == "accepted", "link padding after the datagram is ignored"
}

test test_an_offloaded_udp_checksum_does_not_reject_a_frame {
  let partial = good() |> patched(_, 40, b"\xde\xad")
  assert verdict(partial) == "accepted"
}
