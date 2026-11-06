# linux.netlink_route_request against recorded kernel replies: the fixture
# supplies the reply datagrams, so every reply the real kernel could send, and
# every one it should never send, runs through the same validation as a live
# socket. The fake sends sequence 1 from port id 4242.

const SEQUENCE = 1
const PORT = 4242
const RTM_NEWADDR = 20
const RTM_GETLINK = 18
const NLM_F_REQUEST_ACK = 5
const NLM_F_CREATE_EXCL = 1536
const NLMSG_ERROR = 2
const NLMSG_DONE = 3
const NLM_F_MULTI = 2
const NLM_F_CAPPED = 256
const NLM_F_ACK_TLVS = 512
const SENT_FLAGS = 1541
const ADDRESS = b"\x02\x18\x00\x00\x05\x00\x00\x00"

pure message(kind: Int, flags: Int, sequence: Int, port: Int, payload: Bytes) -> Result[Bytes, Error] {
  Ok(bytes.concat([
    bytes.pack_le(payload.len() + 16, 4)?,
    bytes.pack_le(kind, 2)?,
    bytes.pack_le(flags, 2)?,
    bytes.pack_le(sequence, 4)?,
    bytes.pack_le(port, 4)?,
    payload,
  ]))
}

pure error_payload(code: Int, request_type: Int, sequence: Int, port: Int) -> Result[Bytes, Error] {
  let unsigned = if code == 0 { 0 } else { 4294967296 - code }
  Ok(bytes.concat([bytes.pack_le(unsigned, 4)?, message(request_type, SENT_FLAGS, sequence, port, b"")?]))
}

pure acknowledgement(errno: Int, request_type: Int, sequence: Int, port: Int) -> Result[Bytes, Error] {
  message(NLMSG_ERROR, NLM_F_CAPPED, sequence, port, error_payload(errno, request_type, sequence, port)?)
}

proc failure_of(result: Result[Any, Error], kind: Str) [error] -> Error {
  match result {
    Ok(_) => {
      assert false, "operation unexpectedly succeeded"
      error.failure("unreachable")
    }
    Err(failure) => {
      test.error_kind(failure, kind)
      failure
    }
  }
}

proc install(ctx: TestContext, name: Str, lines: List[Any]) [fs, process, error] -> Result[Path, Error] {
  let fixture = test.temp_file(ctx, name: f"{name}.jsonl", contents: b"")?
  fixture.write(json.encode_lines(lines)?)
  let log = test.temp_file(ctx, name: f"{name}.log", contents: b"")?
  test.linux_fake(ctx, {netlink_fixture: fixture, log: log})
  log
}

proc request() [process, error] -> Result[LinuxNetlinkAck, Error] {
  linux.netlink_route_request(RTM_NEWADDR, NLM_F_CREATE_EXCL, ADDRESS, 1000)
}

proc install_reply(ctx: TestContext, name: Str, datagrams: List[Bytes]) [fs, process, error] -> Result[Path, Error] {
  install(ctx, name, [{op: "netlink_route_request", type: RTM_NEWADDR, datagrams: [d.base64() for d in datagrams]}])
}

test test_acknowledgement_carries_the_numbers_the_call_assigned { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let log = install(ctx, "ack", [
    {
      op: "netlink_route_request",
      type: RTM_NEWADDR,
      flags: SENT_FLAGS,
      payload: ADDRESS.base64(),
      datagrams: [acknowledgement(0, RTM_NEWADDR, SEQUENCE, PORT)?.base64()],
    },
  ])?
  let ack = request()?
  assert ack == {errno: 0, request_type: RTM_NEWADDR, sequence: SEQUENCE, port_id: PORT}
  let calls = log.read_lines()?
  assert calls.len() == 1
  assert "\"op\":\"netlink_route_request\"" in calls[0]
  assert "\"flags\":\"1541\"" in calls[0] and "\"sequence\":\"1\"" in calls[0] and "\"port_id\":\"4242\"" in calls[0]
  assert f"\"payload\":\"{ADDRESS.base64()}\"" in calls[0]
}

test test_kernel_refusal_is_data_not_a_failure { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let _ = install_reply(ctx, "refusal", [acknowledgement(17, RTM_NEWADDR, SEQUENCE, PORT)?])?
  let ack = request()?
  assert ack.errno == 17
  assert ack.request_type == RTM_NEWADDR and ack.sequence == SEQUENCE and ack.port_id == PORT
}

test test_reply_for_another_sequence_or_port_is_rejected { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let outer_sequence = message(NLMSG_ERROR, NLM_F_CAPPED, 2, PORT, error_payload(0, RTM_NEWADDR, SEQUENCE, PORT)?)?
  let echoed_sequence = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, PORT, error_payload(0, RTM_NEWADDR, 2, PORT)?)?
  let outer_port = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, 4243, error_payload(0, RTM_NEWADDR, SEQUENCE, PORT)?)?
  let echoed_port = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, PORT, error_payload(0, RTM_NEWADDR, SEQUENCE, 4243)?)?
  for item in [{name: "outer-sequence", datagram: outer_sequence}, {name: "echoed-sequence", datagram: echoed_sequence}] {
    let _ = install_reply(ctx, item.name, [item.datagram])?
    let _ = failure_of(request(), "linux-netlink-wrong-sequence")
  }
  for item in [{name: "outer-port", datagram: outer_port}, {name: "echoed-port", datagram: echoed_port}] {
    let _ = install_reply(ctx, item.name, [item.datagram])?
    let _ = failure_of(request(), "linux-netlink-wrong-port")
  }
}

test test_multipart_replies_are_rejected { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let body = error_payload(0, RTM_NEWADDR, SEQUENCE, PORT)?
  let flagged = message(NLMSG_ERROR, NLM_F_MULTI, SEQUENCE, PORT, body)?
  let terminator = message(NLMSG_DONE, 0, SEQUENCE, PORT, bytes.zero(4)?)?
  let two_messages = bytes.concat([acknowledgement(0, RTM_NEWADDR, SEQUENCE, PORT)?, acknowledgement(0, RTM_NEWADDR, SEQUENCE, PORT)?])
  for item in [{name: "flagged", datagram: flagged}, {name: "terminator", datagram: terminator}, {name: "two-messages", datagram: two_messages}] {
    let _ = install_reply(ctx, item.name, [item.datagram])?
    let _ = failure_of(request(), "linux-netlink-multipart-reply")
  }
}

test test_truncated_and_inconsistent_replies_are_malformed { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let good = acknowledgement(0, RTM_NEWADDR, SEQUENCE, PORT)?
  let short_payload = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, PORT, bytes.zero(12)?)?
  let longer_than_datagram = bytes.concat([bytes.pack_le(good.len() + 8, 4)?, good[4..good.len()]])
  let shorter_than_header = bytes.concat([bytes.pack_le(8, 4)?, good[4..good.len()]])
  let data_message = message(RTM_NEWADDR, 0, SEQUENCE, PORT, ADDRESS)?
  let positive_code = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, PORT, bytes.concat([bytes.pack_le(17, 4)?, message(RTM_NEWADDR, SENT_FLAGS, SEQUENCE, PORT, b"")?]))?
  let extended = message(NLMSG_ERROR, NLM_F_ACK_TLVS, SEQUENCE, PORT, error_payload(0, RTM_NEWADDR, SEQUENCE, PORT)?)?
  let other_request = acknowledgement(0, RTM_GETLINK, SEQUENCE, PORT)?
  let stray = bytes.concat([good, b"\x01\x02"])
  for item in [
    {name: "empty", datagram: b""},
    {name: "truncated-header", datagram: good[0..10]},
    {name: "short-payload", datagram: short_payload},
    {name: "length-beyond-datagram", datagram: longer_than_datagram},
    {name: "length-below-header", datagram: shorter_than_header},
    {name: "data-message", datagram: data_message},
    {name: "positive-code", datagram: positive_code},
    {name: "extended-acknowledgement", datagram: extended},
    {name: "other-request-type", datagram: other_request},
    {name: "stray-bytes", datagram: stray},
  ] {
    let _ = install_reply(ctx, item.name, [item.datagram])?
    let failure = failure_of(request(), "linux-netlink-malformed-reply")
    assert failure.message != ""
  }
}

test test_reply_larger_than_an_acknowledgement_can_be_is_rejected { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let limit = 16 * 1024 + 20
  let at_limit = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, PORT, bytes.concat([error_payload(0, RTM_NEWADDR, SEQUENCE, PORT)?, bytes.zero(limit - 36)?]))?
  assert at_limit.len() == limit
  let _ = install_reply(ctx, "at-limit", [at_limit])?
  assert request()?.errno == 0
  let over = message(NLMSG_ERROR, NLM_F_CAPPED, SEQUENCE, PORT, bytes.concat([error_payload(0, RTM_NEWADDR, SEQUENCE, PORT)?, bytes.zero(limit - 35)?]))?
  assert over.len() == limit + 1
  let _ = install_reply(ctx, "over-limit", [over])?
  let _ = failure_of(request(), "linux-netlink-oversized-reply")
}

test test_missing_reply_expires_the_deadline_and_transport_failures_keep_their_errno { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let _ = install(ctx, "silent", [{op: "netlink_route_request", type: RTM_NEWADDR, datagrams: []}])?
  let silent = failure_of(request(), "linux-netlink-route-request")
  assert silent.errno == 110
  let _ = install(ctx, "refused", [{op: "netlink_route_request", type: RTM_NEWADDR, errno: 105}])?
  let refused = failure_of(request(), "linux-netlink-route-request")
  assert refused.errno == 105
}

test test_unrecorded_requests_and_bad_lines_fail_loudly { |ctx|
  if system.uname()?.sysname != "Linux" {
    test.skip("the linux netlink fake requires Linux")
    return
  }
  let _ = install(ctx, "other-type", [{op: "netlink_route_request", type: RTM_GETLINK, datagrams: []}])?
  let unmatched = failure_of(request(), "linux-netlink-fake")
  assert "no recorded response for route-netlink request type 20 flags 1541" in unmatched.message
  let _ = install(ctx, "bad-datagram", [{op: "netlink_route_request", datagrams: ["***"]}])?
  let bad = failure_of(request(), "linux-netlink-fake")
  assert "`datagrams` must be a list of base64 text" in bad.message
  let _ = install(ctx, "bad-field", [{op: "netlink_route_request", replies: []}])?
  let unknown = failure_of(request(), "linux-netlink-fake")
  assert "unknown field `replies` for netlink_route_request" in unknown.message
}

test test_request_arguments_are_bounded_before_anything_is_sent {
  test.error_kind(linux.netlink_route_request(3, 0, b"", 1000), "invalid-argument")
  test.error_kind(linux.netlink_route_request(70000, 0, b"", 1000), "invalid-argument")
  test.error_kind(linux.netlink_route_request(RTM_NEWADDR, 768, ADDRESS, 1000), "invalid-argument")
  test.error_kind(linux.netlink_route_request(RTM_NEWADDR, 8, ADDRESS, 1000), "invalid-argument")
  test.error_kind(linux.netlink_route_request(RTM_NEWADDR, 0, ADDRESS, 0), "invalid-argument")
  test.error_kind(linux.netlink_route_request(RTM_NEWADDR, 0, ADDRESS, 60001), "invalid-argument")
  test.error_kind(linux.netlink_route_request(RTM_NEWADDR, 0, bytes.zero(16 * 1024 - 15)?, 1000), "invalid-argument")
}
