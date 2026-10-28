use core.lib.dhcp_arp as arp

const MAC = b"\x02\x00\x00\x00\x00\x01"
const OTHER = b"\x02\x00\x00\x00\x00\x09"
const CANDIDATE = 3232235876

test test_probe_frame_has_the_rfc_5227_layout {
  let frame = arp.probe(MAC, CANDIDATE)?
  assert frame.len() == 60
  assert frame[..6] == b"\xff\xff\xff\xff\xff\xff"
  assert frame[6..12] == MAC
  assert frame[12..14] == b"\x08\x06"
  assert frame[14..22] == b"\x00\x01\x08\x00\x06\x04\x00\x01"
  assert frame[22..28] == MAC
  assert frame[28..32] == b"\x00\x00\x00\x00"
  assert frame[32..38] == b"\x00\x00\x00\x00\x00\x00"
  assert frame[38..42] == b"\xc0\xa8\x01\x64"
  assert frame[42..] == bytes.zero(18)?
}

test test_announce_frame_names_the_address_twice {
  let frame = arp.announce(MAC, CANDIDATE)?
  assert frame[28..32] == b"\xc0\xa8\x01\x64"
  assert frame[38..42] == b"\xc0\xa8\x01\x64"
  guard let packet = arp.parse(frame) else {
    assert false, "announcement did not parse"
    return
  }
  assert packet.operation == 1 and packet.sender_ip == CANDIDATE and packet.target_ip == CANDIDATE
}

test test_a_frame_from_another_host_claiming_the_address_conflicts {
  let claim = arp.announce(OTHER, CANDIDATE)?
  assert arp.conflicts(claim, MAC, CANDIDATE)
  let their_probe = arp.probe(OTHER, CANDIDATE)?
  assert arp.conflicts(their_probe, MAC, CANDIDATE)
}

test test_own_unrelated_and_malformed_frames_do_not_conflict {
  assert ! arp.conflicts(arp.probe(MAC, CANDIDATE)?, MAC, CANDIDATE)
  assert ! arp.conflicts(arp.announce(MAC, CANDIDATE)?, MAC, CANDIDATE)
  assert ! arp.conflicts(arp.announce(OTHER, CANDIDATE + 1)?, MAC, CANDIDATE)
  assert ! arp.conflicts(arp.probe(OTHER, CANDIDATE + 1)?, MAC, CANDIDATE)
  assert ! arp.conflicts(b"", MAC, CANDIDATE)
  let claim = arp.announce(OTHER, CANDIDATE)?
  for length in range(42) {
    assert ! arp.conflicts(claim[..length], MAC, CANDIDATE)
  }
  let not_arp = bytes.concat([claim[..12], b"\x08\x00", claim[14..]])
  assert ! arp.conflicts(not_arp, MAC, CANDIDATE)
  let bad_operation = bytes.concat([claim[..20], b"\x00\x05", claim[22..]])
  assert ! arp.conflicts(bad_operation, MAC, CANDIDATE)
  let ipv6_type = bytes.concat([claim[..16], b"\x86\xdd", claim[18..]])
  assert ! arp.conflicts(ipv6_type, MAC, CANDIDATE)
}

test test_packet_address_follows_the_sockaddr_ll_layout {
  let bound = arp.packet_address(7, 2054, b"")?
  assert bound == b"\x11\x00\x08\x06\x07\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
  let destination = arp.packet_address(7, 2054, b"\xff\xff\xff\xff\xff\xff")?
  assert destination.len() == 20
  assert destination.byte_at(11) == 6
  assert destination.byte_at(10) == 0
  assert destination[12..18] == b"\xff\xff\xff\xff\xff\xff"
  assert arp.packet_address(7, 2054, bytes.zero(9)?) is Err(_)
  assert arp.arp_socket_protocol() == 1544
}

test test_probe_needs_an_ethernet_address {
  assert arp.probe(b"\x01\x02", CANDIDATE) is Err(_)
}
