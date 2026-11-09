use core.lib.net_state
use tests.support.netlink_replay as replay

pure link_entry(ifindex: Int, name: Str?) -> LinuxNetworkLink {
  {address: b"\x02\x00\x00\x00\x00\x07", attributes: [], broadcast: null, flags: 65603, hardware_type: 1, ifindex, kind: "veth", lower_ifindex: 8, master_ifindex: null, mtu: 1500, name, name_bytes: null, operstate: 6, rx_bytes: 0, tx_bytes: 0}
}

pure address_entry(family: Str, local: Str?, peer_address: Str?, prefix: Int) -> LinuxNetworkAddress {
  {address: peer_address, attributes: [], broadcast: null, family, flags: 128, ifindex: 7, label: null, local, preferred_lifetime_seconds: null, prefix_length: prefix, scope: 0, valid_lifetime_seconds: null}
}

pure route_entry(family: Str, attributes: List[LinuxNetlinkAttribute], nexthops: List[LinuxNetworkNexthop]) -> LinuxNetworkRoute {
  {attributes, destination: null, destination_prefix_length: 0, family, flags: 0, gateway: "192.0.2.1", input_ifindex: null, nexthops, output_ifindex: 7, preferred_source: null, priority: null, protocol: 3, route_type: 1, scope: 0, source: null, source_prefix_length: 0, table: 254, tos: 0}
}

pure issue(object: Str, state: Str, errno: Int?) -> LinuxNetworkIssue {
  {object, message: f"{object} {state}", state, errno, error_kind: state}
}

pure dump(state: Str, issues: List[LinuxNetworkIssue]) -> LinuxNetworkDump {
  {
    state,
    enumeration_succeeded: issues.is_empty(),
    issues,
    links: [link_entry(7, "v0")],
    addresses: [address_entry("inet", "192.0.2.2", "192.0.2.2", 24)],
    routes: [route_entry("inet", [], [])],
    rules: [],
  }
}

pure failure_name(result: Result[net_state.State, net_state.ObserveFailure]) -> Str {
  match result {
    Ok(_) => "ok"
    Err(net_state.ObserveFailure.Unsupported {object, ..}) => f"unsupported {object}"
    Err(net_state.ObserveFailure.Denied {object, ..}) => f"denied {object}"
    Err(net_state.ObserveFailure.Malformed {object, ..}) => f"malformed {object}"
    Err(net_state.ObserveFailure.Truncated {object, ..}) => f"truncated {object}"
    Err(net_state.ObserveFailure.Interrupted {object, ..}) => f"interrupted {object}"
    Err(net_state.ObserveFailure.Limited {object, ..}) => f"limited {object}"
    Err(net_state.ObserveFailure.Failed {object, errno, ..}) => f"failed {object} {errno ?? -1}"
    Err(_) => "other"
  }
}

pure address(local: Str, prefix: Int) -> net_state.Address {
  {ifindex: 7, family: .Inet, local, prefix, peer: null, broadcast: null, scope: 0, flags: 0}
}

pure default_route(gateway: Str, metric: Int) -> net_state.Route {
  {
    family: .Inet,
    table: 254,
    destination: null,
    prefix: 0,
    source: null,
    source_prefix: 0,
    preferred_source: null,
    tos: 0,
    priority: metric,
    protocol: 3,
    scope: 0,
    route_type: 1,
    flags: 0,
    hops: [{ifindex: 7, gateway}],
    other_attributes: [],
  }
}

pure unencodable(change: net_state.Change) -> Bool {
  match net_state.request(change) {
    Err(net_state.ChangeFailure.Unencodable {..}) => true
    else => false
  }
}

test test_a_complete_dump_projects_links_addresses_and_routes_into_typed_records [error] {
  let state = net_state.project(dump("complete", []))?
  assert state.links.len() == 1 and state.links[0].name == "v0" and state.links[0].lower_ifindex == 8
  assert net_state.is_up(state.links[0]) and net_state.has_carrier(state.links[0])
  let expected: List[net_state.Address] = [{ifindex: 7, family: .Inet, local: "192.0.2.2", prefix: 24, peer: null, broadcast: null, scope: 0, flags: 128}]
  let hops: List[net_state.Hop] = [{ifindex: 7, gateway: "192.0.2.1"}]
  assert state.addresses == expected
  assert state.routes[0].hops == hops and state.routes[0].priority == 0
  assert state.unmodeled.is_empty()
}

test test_every_failed_dump_is_its_own_error_and_never_an_empty_state [error] {
  let cases = [
    {issue: issue("link", "interrupted", null), expected: "interrupted link"},
    {issue: issue("route", "truncated", null), expected: "truncated route"},
    {issue: issue("address", "malformed", null), expected: "malformed address"},
    {issue: issue("link", "limited", null), expected: "limited link"},
    {issue: issue("socket", "permission_denied", 13), expected: "denied socket"},
    {issue: issue("socket", "unsupported", 97), expected: "unsupported socket"},
    {issue: issue("route", "kernel_error", 22), expected: "failed route 22"},
  ]
  for item in cases {
    var failed = dump("partial", [item.issue])
    failed.links = []
    failed.addresses = []
    failed.routes = []
    assert failure_name(net_state.project(failed)) == item.expected, item.expected
  }
}

test test_rule_and_counter_issues_do_not_block_an_observation_of_links_addresses_and_routes [error] {
  let observed = dump("partial", [issue("rule", "kernel_error", 95), issue("links.7.rx_bytes", "range_failure", null)])
  assert failure_name(net_state.project(observed)) == "ok"
}

test test_an_incomplete_dump_without_an_issue_is_malformed [error] {
  assert failure_name(net_state.project(dump("partial", []))) == "malformed dump"
}

test test_an_input_interface_selector_marks_a_route_as_customized [error] {
  var observed = dump("complete", [])
  observed.routes = [route_entry("inet", [{kind: 3, data: b"\x09\x00\x00\x00"}], [])]
  let state = net_state.project(observed)?
  assert state.routes[0].other_attributes == [3]
}

test test_a_link_without_a_utf8_name_is_malformed_rather_than_renamed [error] {
  var observed = dump("complete", [])
  observed.links = [link_entry(7, null)]
  assert failure_name(net_state.project(observed)) == "malformed link"
}

test test_ipv6_addresses_take_the_peer_field_and_point_to_point_addresses_keep_their_peer [error] {
  var observed = dump("complete", [])
  observed.addresses = [address_entry("inet6", null, "2001:db8::2", 64), address_entry("inet", "10.0.0.1", "10.0.0.2", 32)]
  let state = net_state.project(observed)?
  assert state.addresses[0].local == "2001:db8::2" and state.addresses[0].peer == null and state.addresses[0].family == .Inet6
  assert state.addresses[1].local == "10.0.0.1" and state.addresses[1].peer == "10.0.0.2"
}

test test_unmodeled_families_are_counted_not_dropped_and_unmodeled_route_attributes_are_named [error] {
  var observed = dump("complete", [])
  observed.routes = [
    route_entry("inet", [{kind: 15, data: b"\xfe\x00\x00\x00"}, {kind: 8, data: b""}, {kind: 12, data: b""}, {kind: 8, data: b""}], []),
    route_entry("af_128", [], []),
    route_entry("af_128", [], []),
    route_entry("inet", [], [{ifindex: 7, flags: 0, hops: 0, gateway: "192.0.2.1"}, {ifindex: 9, flags: 0, hops: 0, gateway: "192.0.2.9"}]),
  ]
  let state = net_state.project(observed)?
  assert state.routes.len() == 2
  assert state.routes[0].other_attributes == [8, 12]
  let hops: List[net_state.Hop] = [{ifindex: 7, gateway: "192.0.2.1"}, {ifindex: 9, gateway: "192.0.2.9"}]
  assert state.routes[1].hops == hops
  assert state.unmodeled == [{object: "route", family: "af_128", count: 2}]
}

test test_address_requests_name_the_exact_local_prefix_and_link [error] {
  let add = net_state.request(net_state.AddAddress(address("192.0.3.3", 24)))?
  assert add.message_type == 20 and add.flags == 1541
  assert add.payload == b"\x02\x18\x00\x00\x07\x00\x00\x00\x08\x00\x01\x00\xc0\x00\x03\x03\x08\x00\x02\x00\xc0\x00\x03\x03"
  let remove = net_state.request(net_state.RemoveAddress({...address("192.0.2.2", 24), broadcast: "192.0.2.255"}))?
  assert remove.message_type == 21 and remove.flags == 5
  assert remove.payload == b"\x02\x18\x00\x00\x07\x00\x00\x00\x08\x00\x01\x00\xc0\x00\x02\x02\x08\x00\x02\x00\xc0\x00\x02\x02"
  let six = net_state.request(net_state.AddAddress({...address("2001:db8::1", 64), family: .Inet6}))?
  assert six.payload.len() == 8 + 20 + 20 and six.payload[0..2] == b"\x0a\x40"
}

test test_route_requests_carry_the_hop_priority_and_table_and_removal_keeps_the_observed_selectors [error] {
  let add = net_state.request(net_state.AddRoute(default_route("192.0.2.1", 101)))?
  assert add.message_type == 24 and add.flags == 1541
  assert add.payload == b"\x02\x00\x00\x00\xfe\x03\x00\x01\x00\x00\x00\x00\x08\x00\x05\x00\xc0\x00\x02\x01\x08\x00\x04\x00\x07\x00\x00\x00\x08\x00\x06\x00\x65\x00\x00\x00\x08\x00\x0f\x00\xfe\x00\x00\x00"
  let remove = net_state.request(net_state.RemoveRoute({...default_route("192.0.2.1", 0), other_attributes: [8]}))?
  assert remove.message_type == 25 and remove.flags == 5
  assert remove.payload == b"\x02\x00\x00\x00\xfe\x03\x00\x01\x00\x00\x00\x00\x08\x00\x05\x00\xc0\x00\x02\x01\x08\x00\x04\x00\x07\x00\x00\x00\x08\x00\x0f\x00\xfe\x00\x00\x00"
}

test test_changes_the_library_cannot_express_exactly_are_refused_before_any_request [error] {
  assert unencodable(net_state.AddRoute({...default_route("192.0.2.1", 0), other_attributes: [8]}))
  assert unencodable(net_state.AddRoute({...default_route("192.0.2.1", 0), hops: [{ifindex: 7, gateway: "192.0.2.1"}, {ifindex: 9, gateway: "192.0.2.9"}]}))
  assert unencodable(net_state.AddRoute({...default_route("192.0.2.1", 0), prefix: 24}))
  assert unencodable(net_state.AddAddress(address("192.0.2.300", 24)))
  assert unencodable(net_state.AddAddress({...address("192.0.2.3", 24), family: .Inet6}))
}

pure header(length: Int, kind: Int) -> Bytes {
  bytes.concat([bytes.pack_le(length, 4) ?? b"", bytes.pack_le(kind, 2) ?? b"", bytes.zero(10) ?? b""])
}

pure decoded(datagram: Bytes) -> List[Int]? {
  if let Ok(kinds) = net_state.notice_types(datagram) { kinds } else { null }
}

test test_notification_datagrams_decode_their_types_and_reject_truncation_and_overruns [error] {
  let two = bytes.concat([header(20, 20), b"\x00\x00\x00\x00", header(16, 25)])
  assert decoded(two) == [20, 25]
  assert decoded(b"") == []
  for broken in [header(20, 20)[0..12], header(64, 20), header(8, 20), header(16, 4)] {
    assert decoded(broken) == null
  }
}

proc in_network_namespace(ctx: TestContext, name: Str, body: Str) [fs, process, error] -> Result[Str?] {
  let root = test.temp_dir(ctx, name:)?
  let source = f"""use lib.nettools_fixture as fixture
use lib.net_state

{body}
"""
  let child = test.temp_file(ctx, name: f"{name}.xsh", contents: bytes.from_text(source))?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let child_env = {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", XSH_MODULE_PATH: ctx.core_dir.display()}
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, child], root, child_env, b"", out, err, timeout: 60s)
  match linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) {
    Ok(status) => {
      assert status.shell_code()? == 0, f"{out.read_text()?}{err.read_text()?}"
      Ok(out.read_text()?)
    }
    Err(failure) => {
      test.skip(f"the kernel refuses a private user and network namespace: {failure.message}")
      Ok(null)
    }
  }
}

test test_changes_converge_on_a_veth_pair_and_leave_foreign_state_alone { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("route netlink is Linux-only")
    return
  }
  let body = """proc main() [fs, process, env, net, io, error] {
  fixture.link_add_veth("v0", "v1", "02:00:00:00:00:02", "02:00:00:00:00:03")
  fixture.address_add4("v0", "198.51.100.9", 24, "198.51.100.255", "")
  let before = net_state.observe()?
  let v0 = [link for link in before.links if link.name == "v0"][0]
  let foreign = net_state.addresses_of(before, v0.ifindex)
  let wanted = [
    net_state.LinkUp(v0.ifindex),
    net_state.AddAddress({ifindex: v0.ifindex, family: .Inet, local: "192.0.2.2", prefix: 24, peer: null, broadcast: "192.0.2.255", scope: 0, flags: 0}),
    net_state.AddAddress({ifindex: v0.ifindex, family: .Inet6, local: "2001:db8::2", prefix: 64, peer: null, broadcast: null, scope: 0, flags: 0}),
    net_state.AddRoute({family: .Inet, table: 254, destination: null, prefix: 0, source: null, source_prefix: 0, preferred_source: null, tos: 0, priority: 100, protocol: 3, scope: 0, route_type: 1, flags: 0, hops: [{ifindex: v0.ifindex, gateway: "192.0.2.1"}], other_attributes: []}),
    net_state.AddRoute({family: .Inet6, table: 254, destination: "2001:db8:5::", prefix: 48, source: null, source_prefix: 0, preferred_source: null, tos: 0, priority: 1024, protocol: 3, scope: 0, route_type: 1, flags: 0, hops: [{ifindex: v0.ifindex, gateway: "2001:db8::1"}], other_attributes: []}),
  ]
  for change in wanted {
    net_state.apply(change)?
  }
  let after = net_state.observe()?
  let up = net_state.link_by_index(after, v0.ifindex)
  print f"up {if let link = up { net_state.is_up(link) } else { false }}"
  for item in [item for item in net_state.addresses_of(after, v0.ifindex) if item.scope == 0] {
    print f"address {item.family} {item.local}/{item.prefix}"
  }
  let owned_routes = [route for route in after.routes if route.protocol == 3 and route.hops.len() == 1 and route.hops[0].ifindex == v0.ifindex]
  for route in owned_routes {
    print f"route {route.family} {route.destination ?? "default"}/{route.prefix} via {route.hops[0].gateway ?? "-"} metric {route.priority}"
  }
  match net_state.apply(wanted[1]) {
    Err(net_state.ChangeFailure.Refused {errno, message}) => print f"again refused {errno} {message}"
    else => print "again accepted"
  }
  for route in owned_routes {
    net_state.apply(net_state.RemoveRoute(route))?
  }
  for item in [item for item in net_state.addresses_of(after, v0.ifindex) if item.local in ["192.0.2.2", "2001:db8::2"]] {
    net_state.apply(net_state.RemoveAddress(item))?
  }
  let cleared = net_state.observe()?
  print f"left {[route for route in cleared.routes if route.protocol == 3].len()} routes"
  let kept = [item for item in net_state.addresses_of(cleared, v0.ifindex) if item.family == .Inet]
  print f"foreign kept {kept == foreign}"
}
"""
  guard let listing = in_network_namespace(ctx, "net-state-converge", body)? else { return }
  assert listing == """up true
address inet 198.51.100.9/24
address inet 192.0.2.2/24
address inet6 2001:db8::2/64
route inet default/0 via 192.0.2.1 metric 100
route inet6 2001:db8:5::/48 via 2001:db8::1 metric 1024
again refused 17 File exists
left 0 routes
foreign kept true
""", listing
}

test test_a_notification_overrun_is_reported_as_lost_so_the_caller_reobserves { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("route netlink is Linux-only")
    return
  }
  let body = """pure name(notices: net_state.Notices) -> Str {
  match notices {
    net_state.Quiet => "quiet"
    net_state.Changed => "changed"
    net_state.Lost(reason) => f"lost: {reason}"
  }
}

proc main() [fs, process, env, net, io, error] {
  fixture.link_add_dummy("d0", "02:00:00:00:00:01")
  let d0 = [link for link in net_state.observe()?.links if link.name == "d0"][0]
  let fd = net_state.subscribe()?
  defer unix.close_fd(fd)
  print name(net_state.drain(fd)?)
  net_state.apply(net_state.LinkUp(d0.ifindex))?
  print name(net_state.drain(fd)?)
  print name(net_state.drain(fd)?)
  let c = linux.net_constants()
  linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_RCVBUF, 1)?
  for index in range(64) {
    net_state.apply(net_state.AddAddress({ifindex: d0.ifindex, family: .Inet, local: f"10.1.{index}.1", prefix: 24, peer: null, broadcast: null, scope: 0, flags: 0}))?
  }
  print name(net_state.drain(fd)?)
  let observed = net_state.addresses_of(net_state.observe()?, d0.ifindex)
  print f"reobserved {[item for item in observed if item.family == .Inet].len()}"
}
"""
  guard let listing = in_network_namespace(ctx, "net-state-lost", body)? else { return }
  assert listing == "quiet\nchanged\nquiet\nlost: kernel dropped notifications (ENOBUFS)\nreobserved 64\n", listing
}

const OBSERVE_CHANGE_OBSERVE = """proc show(result: Result[net_state.State, net_state.ObserveFailure]) [io] {
  match result {
    Ok(state) => print f"v0 {net_state.addresses_of(state, 2).len()} addresses"
    Err(net_state.ObserveFailure.Denied {object, ..}) => print f"denied {object}"
    Err(net_state.ObserveFailure.Malformed {object, ..}) => print f"malformed {object}"
    Err(net_state.ObserveFailure.Limited {object, ..}) => print f"limited {object}"
    Err(net_state.ObserveFailure.Failed {object, errno, message}) => print f"failed {object} {errno ?? -1}: {message}"
    Err(_) => print "other"
  }
}

proc main() [process, env, net, io, error] {
  show(net_state.observe())
  match net_state.apply(net_state.AddAddress({ifindex: 2, family: .Inet, local: "192.0.2.2", prefix: 24, peer: null, broadcast: null, scope: 0, flags: 0})) {
    Ok(_) => print "applied"
    Err(net_state.ChangeFailure.Refused {errno, ..}) => print f"refused {errno}"
    Err(net_state.ChangeFailure.Transport {message}) => print f"transport: {message}"
    Err(_) => print "unencodable"
  }
  show(net_state.observe())
}
main()?
"""

type ReplayedDump = {op: Str, links: Any, addresses: Any, routes: Any, rules: Any}
type ReplayedChange = {op: Str, type: Int, datagrams: List[Str]}
type Replayed = {success: Bool, stdout: Str, stderr: Str, calls: Str}

proc before_dump() [error] -> Result[ReplayedDump] {
  let mac = b"\x02\x00\x00\x00\x00\x07"
  {
    op: "network_dump",
    links: replay.dump(1, [replay.link(2, 65603, "v0", mac, 1500)?])?,
    addresses: replay.dump(2, [])?,
    routes: replay.dump(3, [])?,
    rules: replay.dump(4, [])?,
  }
}

proc after_dump() [error] -> Result[ReplayedDump] {
  let mac = b"\x02\x00\x00\x00\x00\x07"
  {
    op: "network_dump",
    links: replay.dump(1, [replay.link(2, 65603, "v0", mac, 1500)?])?,
    addresses: replay.dump(2, [replay.address4(2, 24, b"\xc0\x00\x02\x02")?])?,
    routes: replay.dump(3, [])?,
    rules: replay.dump(4, [])?,
  }
}

proc address_change_reply(errno: Int) [error] -> Result[ReplayedChange] {
  {op: "netlink_route_request", type: 20, datagrams: [replay.acknowledgement(errno)?]}
}

proc replayed(ctx: TestContext, name: Str, lines: List[Any]) [fs, process, error] -> Result[Replayed] {
  let root = test.temp_dir(ctx, name:)?
  let file = fp"{root}/replay.jsonl"
  file.write(json.encode_lines(lines)?)
  let log = fp"{root}/calls.jsonl"
  test.linux_fake(ctx, {network_replay: file, log})
  let ran = test.run_script(ctx, f"use lib.net_state\n\n{OBSERVE_CHANGE_OBSERVE}", [], {XSH_MODULE_PATH: ctx.core_dir}, b"", name)?
  {success: ran.success, stdout: ran.stdout, stderr: ran.stderr, calls: if log.exists()? { log.read_text()? } else { "" }}
}

test test_a_fixed_replay_drives_observe_change_observe_without_the_host { |ctx|
  let lines: List[Any] = [before_dump()?, address_change_reply(0)?, after_dump()?]
  let first = replayed(ctx, "replay-ok", lines)?
  assert first.success, first.stderr
  assert first.stdout == "v0 0 addresses\napplied\nv0 1 addresses\n", first.stdout
  let kinds = [line.split("\"op\":\"")[1].split("\"")[0] for line in first.calls.lines()]
  assert kinds == ["network_dump", "netlink_route_request", "network_dump"], first.calls
  let second = replayed(ctx, "replay-again", lines)?
  assert second.stdout == first.stdout
}

test test_an_omitted_reordered_or_stale_observation_fails_explicitly { |ctx|
  let omitted = replayed(ctx, "replay-omitted", [before_dump()?, address_change_reply(0)?])?
  assert "`network_dump` after all 2 replayed observations were consumed" in omitted.stdout, omitted.stdout

  let reordered = replayed(ctx, "replay-reordered", [address_change_reply(0)?, before_dump()?, after_dump()?])?
  assert "replay observation 1: expected `netlink_route_request`, observed `network_dump`" in reordered.stdout, reordered.stdout

  let stale = replayed(ctx, "replay-stale", [before_dump()?, address_change_reply(0)?, after_dump()?, after_dump()?])?
  assert ! stale.success
  assert "replay incomplete: 3 of 4 observations were consumed" in stale.stderr, stale.stderr

  let wrong_request = replayed(ctx, "replay-wrong-request", [before_dump()?, {...address_change_reply(0)?, type: 21}, after_dump()?])?
  assert "does not match the recorded request" in wrong_request.stdout, wrong_request.stdout
}

test test_injected_denials_short_reads_and_refusals_never_become_a_state { |ctx|
  let denied = replayed(ctx, "replay-denied", [{...before_dump()?, links: 1}, address_change_reply(0)?, after_dump()?])?
  assert denied.stdout.starts_with("denied link\n"), denied.stdout

  let short = replayed(ctx, "replay-short", [{...before_dump()?, addresses: [replay.dump(2, [])?[0]]}, address_change_reply(0)?, after_dump()?])?
  assert short.stdout.starts_with("limited address\n"), short.stdout

  let mac = b"\x02\x00\x00\x00\x00\x07"
  let torn = replay.dump(1, [replay.link(2, 65603, "v0", mac, 1500)?])?[0].base64_decode()?
  let truncated = replayed(ctx, "replay-truncated", [{...before_dump()?, links: [torn[0..30].base64()]}, address_change_reply(0)?, after_dump()?])?
  assert truncated.stdout.starts_with("malformed link\n"), truncated.stdout

  let refused = replayed(ctx, "replay-refused", [before_dump()?, address_change_reply(17)?, after_dump()?])?
  assert refused.stdout == "v0 0 addresses\nrefused 17\nv0 1 addresses\n", refused.stdout

  let silent = replayed(ctx, "replay-silent", [before_dump()?, {op: "netlink_route_request"}, after_dump()?])?
  assert "transport:" in silent.stdout, silent.stdout
}

test test_the_same_scenario_on_a_live_namespace_matches_the_replay { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("route netlink is Linux-only")
    return
  }
  let body = OBSERVE_CHANGE_OBSERVE.replace("main()?\n", with: "fixture.link_add_dummy(\"v0\", \"02:00:00:00:00:07\")?\nmain()?\n")
  guard let listing = in_network_namespace(ctx, "net-replay-live", body)? else { return }
  assert listing == "v0 0 addresses\napplied\nv0 1 addresses\n", listing
}
