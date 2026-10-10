# The netlink fixture of the linux test fake: recorded generic-netlink
# exchanges answer the netlink primitives, so scripts that talk to hardware
# the test host lacks (wireless devices) run unchanged against the fake.
# Nothing here reaches a kernel except the checks that a descriptor the fake
# did not open still takes the real path.

pure errno_of(result: Result[Any, Error]) -> Int {
  if let Err(failure) = result {
    return failure.errno ?? -1
  } else {
    return 0
  }
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

# Installs the fake with `lines` as the recorded kernel and returns the call
# log path.
proc install(ctx: TestContext, name: Str, lines: List[Any]) [fs, process, error] -> Result[Path, Error] {
  let fixture = test.temp_file(ctx, name: f"{name}.jsonl", contents: b"")?
  fixture.write(json.encode_lines(lines)?)
  let log = test.temp_file(ctx, name: f"{name}.log", contents: b"")?
  test.linux_fake(ctx, {netlink_fixture: fixture, log: log})?
  log
}

const REPLY = "AQIDBA=="
const EVENT = "BQYHCA=="

test test_fixture_answers_family_lookups_and_requests { |ctx|
  let log = install(ctx, "requests", [
    {op: "genl_family", name: "nl80211", id: 28},
    {op: "genl_family", name: "absent", errno: 2},
    {
      op: "netlink_request",
      protocol: 16,
      type: 28,
      cmd: 5,
      flags: 773,
      replies: [{type: 28, flags: 2, payload: REPLY}, {type: 28, payload: EVENT}],
    },
    {op: "netlink_request", type: 28, cmd: 31, errno: 1},
    {op: "netlink_request", type: 28, payload: EVENT, replies: []},
  ])?
  assert linux.genl_family_id("nl80211")? == 28
  assert errno_of(linux.genl_family_id("absent")) == 2
  let nl = linux.netlink_open(16)?
  let replies = linux.netlink_request(nl, 28, 773, b"\x05\x00\x00\x00")?
  assert replies.len() == 2
  assert replies[0].type == 28 and replies[0].flags == 2 and replies[0].payload == b"\x01\x02\x03\x04"
  assert replies[1].flags == 0 and replies[1].payload == b"\x05\x06\x07\x08"
  assert replies[0].seq != 0
  assert errno_of(linux.netlink_request(nl, 28, 5, b"\x1f\x00\x00\x00")) == 1
  # A line with `payload` matches only those exact request bytes.
  assert linux.netlink_request(nl, 28, 5, b"\x05\x06\x07\x08")?.len() == 0
  unix.close_fd(nl)?

  let calls = log.read_text()?.lines()
  assert "\"op\":\"genl_family_id\"" in calls[0] and "\"name\":\"nl80211\"" in calls[0]
  assert "\"op\":\"netlink_open\"" in calls[2] and "\"protocol\":\"16\"" in calls[2]
  assert "\"op\":\"netlink_request\"" in calls[3]
  assert "\"type\":\"28\"" in calls[3] and "\"flags\":\"773\"" in calls[3]
  assert "\"payload\":\"BQAAAA==\"" in calls[3]
}

test test_unrecorded_requests_and_bad_fixture_lines_fail_loudly { |ctx|
  let _ = install(ctx, "unrecorded", [
    {op: "genl_family", name: "nl80211", id: 28},
    {op: "netlink_request", type: 28, cmd: 5, replies: []},
  ])?
  let nl = linux.netlink_open(16)?
  let failure = failure_of(linux.netlink_request(nl, 28, 5, b"\x06\x00\x00\x00"), "linux-netlink-fake")
  assert "no recorded response for netlink request" in failure.message
  let _ = failure_of(linux.genl_family_id("other"), "linux-netlink-fake")
  unix.close_fd(nl)?

  let _ = install(ctx, "bad-field", [{op: "genl_family", name: "x", bogus: 1}])?
  let unknown = failure_of(linux.genl_family_id("x"), "linux-netlink-fake")
  assert "unknown field `bogus` for genl_family" in unknown.message and ":1:" in unknown.message
  let _ = install(ctx, "bad-base64", [{op: "netlink_request", type: 1, payload: "***"}])?
  let nl2 = linux.netlink_open(16)?
  let binary = failure_of(linux.netlink_request(nl2, 1, 1, b""), "linux-netlink-fake")
  assert "`payload` must be base64 text" in binary.message
  unix.close_fd(nl2)?
  let _ = install(ctx, "bad-op", [{op: "other"}])?
  let op = failure_of(linux.genl_family_id("x"), "linux-netlink-fake")
  assert "unknown op `other`" in op.message
}

test test_events_reach_only_sockets_that_joined_their_group { |ctx|
  let log = install(ctx, "events", [
    {op: "netlink_event", type: 28, flags: 0, payload: REPLY, group: 20},
    {op: "netlink_event", type: 28, flags: 2, payload: EVENT, group: 21},
    {op: "netlink_event", type: 29, payload: REPLY},
    {op: "netlink_event", errno: 104},
  ])?
  let c = linux.net_constants()
  let nl = linux.netlink_open(16)?
  # Group ids are joined with the NETLINK_ADD_MEMBERSHIP socket option.
  linux.setsockopt_int(nl, c.SOL_NETLINK, 1, 21)?
  linux.set_socket_timeout(nl, c.SO_RCVTIMEO, 1000)?
  let first = linux.recvfrom(nl, 4096)?
  assert first.data.len() == 20
  assert bytes.unpack_le(first.data, 2, 4)? == 28 and bytes.unpack_le(first.data, 2, 6)? == 2
  assert first.data.slice(16, 4) == b"\x05\x06\x07\x08"
  assert first.address.family == "netlink"
  let second = linux.recvfrom(nl, 4096)?
  assert bytes.unpack_le(second.data, 2, 4)? == 29
  assert errno_of(linux.recvfrom(nl, 4096)) == 104
  # Past the last recorded event the receive would block, as an idle
  # socket with a timeout does.
  assert errno_of(linux.recvfrom(nl, 4096)) == 11
  unix.close_fd(nl)?

  # A socket that joined nothing sees only the events with no group.
  let other = linux.netlink_open(16)?
  let ungrouped = linux.recvfrom(other, 4096)?
  assert bytes.unpack_le(ungrouped.data, 2, 4)? == 29
  unix.close_fd(other)?

  # A truncated receive reports MSG_TRUNC.
  # The group mask of `netlink_open` joins group 20 (bit 19) at creation.
  let third = linux.netlink_open(16, 524288)?
  let cut = linux.recvfrom(third, 8)?
  assert cut.data.len() == 8 and cut.flags.bit_and(32) != 0
  unix.close_fd(third)?
  let calls = log.read_text()?
  assert "\"op\":\"setsockopt\"" in calls and "\"level\":\"270\"" in calls
  assert "\"op\":\"set_socket_timeout\"" in calls
}

test test_descriptors_the_fake_did_not_open_take_the_real_path { |ctx|
  let _ = install(ctx, "real-path", [{op: "genl_family", name: "nl80211", id: 28}])?
  let c = linux.net_constants()
  # A real loopback socket still moves real datagrams while the fake is on.
  let receiver = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(receiver)
  linux.bind(receiver, {family: "inet", address: "127.0.0.1", port: 0})?
  let name = linux.getsockname(receiver)?
  let sender = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(sender)
  let _ = linux.sendto(sender, b"ping", {family: "inet", address: "127.0.0.1", port: name.port})?
  linux.set_socket_timeout(receiver, c.SO_RCVTIMEO, 2000)?
  linux.setsockopt_int(receiver, c.SOL_SOCKET, c.SO_REUSEADDR, 1)?
  assert linux.recvfrom(receiver, 16)?.data == b"ping"
  # A closed descriptor is the kernel's EBADF, not a fake socket.
  let nl = linux.netlink_open(16)?
  unix.close_fd(nl)?
  assert errno_of(linux.recvfrom(nl, 16)) == 9
}

test test_without_a_netlink_fixture_the_fake_leaves_netlink_real { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("netlink is Linux-only")
    return
  }
  let log = test.temp_file(ctx, name: "no-fixture.log", contents: b"")?
  test.linux_fake(ctx, {log: log})?
  assert linux.genl_family_id("nlctrl")? == 16
  assert errno_of(linux.genl_family_id("xsh-no-such")) == 2
}
