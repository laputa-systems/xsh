use lib.dhcp_wire as wire
use tests.support.dhcp_pcap as pcap
use tests.support.dhcp_scenario as scenario
use tests.support.dhcp_script as script
use tests.support.dhcp_types as types

const CLIENT_MAC = b"\x02\x00\x00\x00\x00\x01"
const SERVER = 3232248321
const OFFERED = 3232248420

type LinkRow = {name: Str?, ifindex: Int, flags: Int}

type AddressRow = {ifindex: Int, address: Str, prefix: Int, label: Str?}

type RouteRow = {destination: Str?, prefix: Int, gateway: Str?, ifindex: Int?, table: Int, tos: Int, priority: Int?, protocol: Int, scope: Int}

type Snapshot = {state: Str, links: List[LinkRow], addresses: List[AddressRow], routes: List[RouteRow]}

proc write_rules(ctx: TestContext, rules: List[types.Rule]) [fs, process, error] -> Result[Path, Error] {
  test.temp_file(ctx, name: "server-script.json", contents: bytes.from_text(json.encode({rules: rules})?))
}

pure client_args(extra: List[Str]) -> List[Str] {
  ["--lease-dir", "{dir}/lease", "--run-dir", "{dir}/run", "--resolv-dir", "{dir}/etc", "--static-file", "{dir}/etc/resolv.static", @extra, "eth0"]
}

pure bound_rules() -> List[types.Rule] {
  [
    script.always("discover", [{...script.reply("offer"), lease: 3600}]),
    script.always("request", [{...script.reply("ack"), lease: 3600}]),
  ]
}

test test_the_supervised_client_acquires_a_lease_and_leaves_foreign_state_alone { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "acquire",
    [
      scenario.fixture("link_add_dummy", ["ts0", "02:00:00:00:00:77"]),
      scenario.fixture("link_up", ["ts0"]),
      scenario.fixture("address_add4", ["ts0", "100.100.1.1", "32", "100.100.1.1", ""]),
      scenario.fixture("route_add_type4", ["10.98.0.0", "16", "6"]),
      scenario.capture("client", "client.pcap"),
      scenario.server_xsh(file),
      scenario.wait_for("client.pcap", "", 5000),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", "nameserver 10.0.0.53\n"),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.snapshot("bound"),
      scenario.sleep(2600),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let bound = json.read(fp"{dir}/bound.json")?.require(Snapshot)?
  let addresses = [item.address for item in bound.addresses]
  assert "192.168.50.100" in addresses, addresses.join(",")
  assert "100.100.1.1" in addresses, "the foreign address on the other link survives"
  let default_routes = [route for route in bound.routes if route.destination == null and route.gateway == "192.168.50.1"]
  assert default_routes.len() == 1 and default_routes[0].protocol == 16 and default_routes[0].priority == 100 and default_routes[0].tos == 0
  assert [route for route in bound.routes if route.destination == "10.98.0.0"].len() == 1, "the foreign route survives"
  let resolv = fp"{dir}/etc/resolv.conf".read_text()?
  assert resolv.starts_with("# Managed by dhcpc") and "nameserver 10.0.0.53\nnameserver 192.168.50.1\n" in resolv, resolv

  let captured = pcap.dhcp_messages(pcap.parse(fp"{dir}/client.pcap".read_bytes()?)?)
  assert [item.message.kind for item in captured] == [1, 2, 3, 5]
  let discover = captured[0]
  assert discover.frame.source_mac == CLIENT_MAC and discover.frame.destination_mac == b"\xff\xff\xff\xff\xff\xff"
  assert discover.frame.ip?.source == 0 and discover.frame.ip?.destination == 4294967295 and discover.frame.ip?.checksum_ok == true
  assert discover.frame.udp?.source_port == 68 and discover.frame.udp?.destination_port == 67
  assert discover.message.broadcast and discover.message.client_id == bytes.concat([b"\x01", CLIENT_MAC])
  assert discover.message.parameter_list == wire.requested_options()
  let request = captured[2]
  assert request.message.requested_ip == OFFERED and request.message.server_id == SERVER and request.message.ciaddr == 0
  assert request.message.xid == discover.message.xid
  let frames = [pcap.dissect(entry.frame) for entry in pcap.parse(fp"{dir}/client.pcap".read_bytes()?)?]
  let probes = [frame for frame in frames if frame.ethertype == 2054 and frame.arp?.sender_ip == 0 and frame.arp?.target_ip == OFFERED]
  let announces = [frame for frame in frames if frame.ethertype == 2054 and frame.arp?.sender_ip == OFFERED and frame.arp?.target_ip == OFFERED]
  assert probes.len() == 3 and announces.len() == 2, f"{probes.len()} probes, {announces.len()} announcements"
}

const SERVER_MAC = b"\x02\x00\x00\x00\x00\x02"

test test_renewal_is_unicast_at_t1_rebinding_is_broadcast_at_t2_and_expiry_withdraws_the_lease { |ctx|
  let rules = [
    script.always("discover", [{...script.reply("offer"), lease: 20}]),
    script.only("request", 1, [{...script.reply("ack"), lease: 20}]),
    script.from("request", 2, []),
  ]
  let file = write_rules(ctx, rules)?
  guard let dir = scenario.execute(
    ctx,
    "lifecycle",
    [
      scenario.capture("client", "client.pcap"),
      scenario.server_xsh(file),
      scenario.wait_for("client.pcap", "", 5000),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", ""),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.snapshot("bound"),
      scenario.sleep(22000),
      scenario.snapshot("expired"),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let bound = json.read(fp"{dir}/bound.json")?.require(Snapshot)?
  assert "192.168.50.100" in [item.address for item in bound.addresses]
  let expired = json.read(fp"{dir}/expired.json")?.require(Snapshot)?
  assert "192.168.50.100" not in [item.address for item in expired.addresses], "expiry withdraws the owned address"
  assert [route for route in expired.routes if route.gateway == "192.168.50.1"].is_empty(), "and the owned default route"

  let captured = pcap.dhcp_messages(pcap.parse(fp"{dir}/client.pcap".read_bytes()?)?)
  let requests = [item for item in captured if item.message.kind == 3]
  let renewals = [item for item in requests if item.message.ciaddr == OFFERED and item.frame.ip?.destination == SERVER]
  let rebindings = [item for item in requests if item.message.ciaddr == OFFERED and item.frame.ip?.destination == 4294967295]
  assert ! renewals.is_empty(), "renewal is unicast to the server"
  assert renewals[0].frame.destination_mac == SERVER_MAC and ! renewals[0].message.broadcast
  assert renewals[0].message.requested_ip == null and renewals[0].message.server_id == null
  assert ! rebindings.is_empty(), "rebinding is broadcast"
  assert rebindings[0].frame.destination_mac == b"\xff\xff\xff\xff\xff\xff"
  assert rebindings[0].at_us > renewals[0].at_us
  let first_request = requests[0].at_us
  let renew_delay = (renewals[0].at_us - first_request) / 1000
  assert renew_delay >= 9500 and renew_delay <= 11500, f"T1 is ten seconds after the request, saw {renew_delay} ms"
  let rebind_delay = (rebindings[0].at_us - first_request) / 1000
  assert rebind_delay >= 17000 and rebind_delay <= 19000, f"T2 is 17.5 seconds after the request, saw {rebind_delay} ms"
  let discovers = [item for item in captured if item.message.kind == 1]
  assert discovers.len() >= 2, "expiry starts discovery again"
}

test test_ifup_and_ifdown_drive_the_running_client_without_a_second_acquisition_or_a_flush { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "lifecycle-contract",
    [
      scenario.fixture("link_add_dummy", ["ts0", "02:00:00:00:00:77"]),
      scenario.fixture("link_up", ["ts0"]),
      scenario.fixture("address_add4", ["ts0", "100.100.1.1", "32", "100.100.1.1", ""]),
      scenario.fixture("link_up", ["eth0"]),
      scenario.fixture("address_add4", ["eth0", "10.99.0.5", "24", "10.99.0.255", ""]),
      scenario.fixture("route_add_type4", ["10.98.0.0", "16", "6"]),
      scenario.write("interfaces", "auto eth0\niface eth0 inet dhcp\n"),
      scenario.write("etc/resolv.static", ""),
      scenario.capture("client", "client.pcap"),
      scenario.server_xsh(file),
      scenario.wait_for("client.pcap", "", 5000),
      scenario.wait_for("server.requests", "", 5000),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.snapshot("bound"),
      scenario.run_applet("dhcpc", "second", client_args([])),
      scenario.run_applet("ifup", "ifup-running", ["eth0", "--wait", "5"]),
      scenario.run_applet("ifdown", "ifdown-running", ["eth0", "--wait", "15"]),
      scenario.snapshot("down"),
      scenario.copy("etc/resolv.conf", "down.resolv"),
      scenario.run_applet("ifup", "ifup-again", ["eth0", "--wait", "15"]),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 25000),
      scenario.snapshot("up"),
      scenario.sleep(300),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  assert fp"{dir}/second.status".read_text()?.trim() != "0", "a second client for the interface must refuse to start"
  assert "another client owns eth0" in fp"{dir}/second.out".read_text()?
  assert fp"{dir}/ifup-running.status".read_text()?.trim() == "0", fp"{dir}/ifup-running.out".read_text()?
  assert fp"{dir}/ifdown-running.status".read_text()?.trim() == "0", fp"{dir}/ifdown-running.out".read_text()?

  let down = json.read(fp"{dir}/down.json")?.require(Snapshot)?
  let down_addresses = [item.address for item in down.addresses]
  assert "192.168.50.100" not in down_addresses, "ifdown withdrew the owned address"
  assert "10.99.0.5" in down_addresses and "100.100.1.1" in down_addresses, "and nothing else"
  assert [route for route in down.routes if route.gateway == "192.168.50.1"].is_empty()
  assert [route for route in down.routes if route.destination == "10.98.0.0"].len() == 1
  assert [link for link in down.links if link.name == "eth0"][0].flags.bit_and(1) == 1, "the link stays up under a supervised client"
  assert fp"{dir}/down.resolv".read_text()? == "# Managed by dhcpc: edit the static file, not this one.\n"

  let up = json.read(fp"{dir}/up.json")?.require(Snapshot)?
  assert "192.168.50.100" in [item.address for item in up.addresses] and "10.99.0.5" in [item.address for item in up.addresses]

  let captured = pcap.dhcp_messages(pcap.parse(fp"{dir}/client.pcap".read_bytes()?)?)
  let kinds = [item.message.kind for item in captured]
  assert kinds == [1, 2, 3, 5, 7, 1, 2, 3, 5], "one acquisition, one release, one more acquisition and no extra discovery from ifup"
  let release = captured[4]
  assert release.frame.ip?.destination == SERVER and release.frame.destination_mac == SERVER_MAC
  assert release.message.ciaddr == OFFERED and release.message.server_id == SERVER and ! release.message.broadcast
}

test test_a_killed_client_restarts_into_init_reboot_and_keeps_the_address { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "crash-restart",
    [
      scenario.capture("client", "client.pcap"),
      scenario.server_xsh(file),
      scenario.wait_for("client.pcap", "", 5000),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", ""),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.kill("client", "KILL"),
      scenario.sleep(300),
      scenario.snapshot("crashed"),
      scenario.client("restarted", client_args([])),
      scenario.sleep(6000),
      scenario.snapshot("restarted"),
      scenario.kill("restarted", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let crashed = json.read(fp"{dir}/crashed.json")?.require(Snapshot)?
  assert "192.168.50.100" in [item.address for item in crashed.addresses], "a crash leaves the configuration in place"
  let restarted = json.read(fp"{dir}/restarted.json")?.require(Snapshot)?
  assert [item.address for item in restarted.addresses if item.address == "192.168.50.100"].len() == 1
  assert [route for route in restarted.routes if route.gateway == "192.168.50.1"].len() == 1
  let captured = pcap.dhcp_messages(pcap.parse(fp"{dir}/client.pcap".read_bytes()?)?)
  assert [item.message.kind for item in captured] == [1, 2, 3, 5, 3, 5], "one discovery, then a validation of the remembered lease"
  let validation = captured[4]
  assert validation.message.ciaddr == 0 and validation.message.requested_ip == OFFERED and validation.message.server_id == null
}

test test_link_flap_and_interface_recreation_are_survived_without_a_new_discovery_storm { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "flap-recreate",
    [
      scenario.fixture("link_add_dummy", ["ts0", "02:00:00:00:00:77"]),
      scenario.fixture("link_up", ["ts0"]),
      scenario.fixture("address_add4", ["ts0", "100.100.1.1", "32", "100.100.1.1", ""]),
      scenario.server_xsh(file),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", ""),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.snapshot("bound"),
      scenario.link("down"),
      scenario.sleep(2000),
      scenario.link("up"),
      scenario.wait_for("run/eth0.status", "\"sent_request_init_reboot\":1", 30000),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 30000),
      scenario.sleep(500),
      scenario.snapshot("flapped"),
      scenario.recreate(),
      scenario.link("up"),
      scenario.server_stop("server"),
      {...scenario.server_xsh(file), name: "again"},
      scenario.wait_for("again.requests", "\"kind\":\"request\"", 30000),
      scenario.sleep(3000),
      scenario.snapshot("recreated"),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let bound = json.read(fp"{dir}/bound.json")?.require(Snapshot)?
  let flapped = json.read(fp"{dir}/flapped.json")?.require(Snapshot)?
  assert "192.168.50.100" in [item.address for item in flapped.addresses]
  assert [route for route in flapped.routes if route.gateway == "192.168.50.1"].len() == 1, "the route the kernel dropped with the carrier is back"
  let recreated = json.read(fp"{dir}/recreated.json")?.require(Snapshot)?
  let old_index = [link for link in bound.links if link.name == "eth0"][0].ifindex
  let new_index = [link for link in recreated.links if link.name == "eth0"][0].ifindex
  assert new_index != old_index, "the interface really was recreated"
  let on_new = [item for item in recreated.addresses if item.ifindex == new_index]
  assert [item.address for item in on_new] == ["192.168.50.100"], "the lease is configured on the new device"
  assert [route for route in recreated.routes if route.gateway == "192.168.50.1" and route.ifindex == new_index].len() == 1
  assert "100.100.1.1" in [item.address for item in recreated.addresses], "foreign state is untouched"
  let again = fp"{dir}/again.requests".read_text()?
  assert "\"kind\":\"discover\"" not in again, "the remembered lease is validated, not discarded"
}

test test_a_foreign_resolver_file_is_reported_and_left_alone_while_the_lease_still_binds { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "resolver-conflict",
    [
      scenario.server_xsh(file),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", ""),
      scenario.write("etc/resolv.conf", "nameserver 9.9.9.9\n"),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.sleep(1000),
      scenario.snapshot("bound"),
      scenario.copy("run/eth0.status", "bound.status"),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  assert fp"{dir}/etc/resolv.conf".read_text()? == "nameserver 9.9.9.9\n"
  let status = fp"{dir}/bound.status".read_text()?
  assert "dns-conflict" in status and "foreign_file" in status, status
  let bound = json.read(fp"{dir}/bound.json")?.require(Snapshot)?
  assert "192.168.50.100" in [item.address for item in bound.addresses], "a resolver conflict does not undo the address"
}

test test_the_client_recovers_after_a_silent_server_returns { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "server-absence",
    [
      scenario.write("etc/resolv.static", ""),
      scenario.client("client", client_args([])),
      scenario.sleep(9000),
      scenario.server_xsh(file),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 40000),
      scenario.snapshot("bound"),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let bound = json.read(fp"{dir}/bound.json")?.require(Snapshot)?
  assert "192.168.50.100" in [item.address for item in bound.addresses]
}

type Sample = {ticks: Int, rss_kb: Int, voluntary_switches: Int, involuntary_switches: Int, fds: Int, threads: Int}

type Measurement = {window_ms: Int, before: Sample, after: Sample}

test test_a_bound_client_idles_without_polling_or_growth { |ctx|
  let file = write_rules(ctx, bound_rules())?
  guard let dir = scenario.execute(
    ctx,
    "idle-cost",
    [
      scenario.server_xsh(file),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", ""),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.sleep(3000),
      scenario.measure("client", 20000),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let measured = json.read(fp"{dir}/measure-client.json")?.require(Measurement)?
  let cpu_ms = (measured.after.ticks - measured.before.ticks) * 10
  let wakeups = measured.after.voluntary_switches - measured.before.voluntary_switches + measured.after.involuntary_switches - measured.before.involuntary_switches
  assert cpu_ms <= 100, f"{cpu_ms} ms of CPU in {measured.window_ms} ms"
  assert wakeups <= 20, f"{wakeups} context switches in {measured.window_ms} ms"
  assert measured.after.fds == measured.before.fds, "no descriptor leaks while idle"
  assert measured.after.rss_kb <= measured.before.rss_kb + 512, "no memory growth while idle"
}

pure owned_by_client(snapshot: Snapshot) -> List[RouteRow] {
  [route for route in snapshot.routes if route.protocol == 16]
}

test test_classless_static_routes_replace_the_router_are_owned_and_follow_the_renewal { |ctx|
  let first = ["10.77.0.0/16 192.168.50.1", "192.0.2.9/32 0.0.0.0", "198.51.100.0/24 192.0.2.9", "0.0.0.0/0 192.168.50.1"]
  let second = ["10.88.0.0/16 192.168.50.1", "192.0.2.9/32 0.0.0.0", "0.0.0.0/0 192.168.50.1"]
  let rules = [
    script.always("discover", [{...script.reply("offer"), lease: 20}]),
    script.only("request", 1, [{...script.reply("ack"), lease: 20, classless: first}]),
    script.from("request", 2, [{...script.reply("ack"), lease: 20, classless: second}]),
  ]
  let file = write_rules(ctx, rules)?
  guard let dir = scenario.execute(
    ctx,
    "classless",
    [
      scenario.fixture("link_add_dummy", ["ts0", "02:00:00:00:00:77"]),
      scenario.fixture("link_up", ["ts0"]),
      scenario.fixture("route_add_type4", ["10.98.0.0", "16", "6"]),
      scenario.server_xsh(file),
      scenario.wait_for("server.requests", "", 5000),
      scenario.write("etc/resolv.static", ""),
      scenario.client("client", client_args([])),
      scenario.wait_for("run/eth0.status", "\"state\":\"bound\"", 20000),
      scenario.sleep(500),
      scenario.snapshot("first"),
      scenario.wait_for("server.requests", "\"occurrence\":2", 25000),
      scenario.sleep(1500),
      scenario.snapshot("second"),
      scenario.kill("client", "TERM"),
      scenario.sleep(500),
    ],
  )? else { return }
  let initial = owned_by_client(json.read(fp"{dir}/first.json")?.require(Snapshot)?)
  let shapes = [f"{route.destination ?? "default"}/{route.prefix} via {route.gateway ?? "link"}" for route in initial]
  assert shapes.len() == 4, shapes.join(",")
  for expected in ["10.77.0.0/16 via 192.168.50.1", "192.0.2.9/32 via link", "198.51.100.0/24 via 192.0.2.9", "default/0 via 192.168.50.1"] {
    assert expected in shapes, f"{expected} missing from {shapes.join(",")}"
  }
  let on_link = [route for route in initial if route.gateway == null][0]
  assert on_link.scope == 253, "an on-link route has link scope"
  assert [route for route in initial if route.priority != 100].is_empty()

  let renewed = json.read(fp"{dir}/second.json")?.require(Snapshot)?
  let later = [f"{route.destination ?? "default"}/{route.prefix} via {route.gateway ?? "link"}" for route in owned_by_client(renewed)]
  assert later.len() == 3, later.join(",")
  assert "10.88.0.0/16 via 192.168.50.1" in later and "10.77.0.0/16 via 192.168.50.1" not in later
  assert "198.51.100.0/24 via 192.0.2.9" not in later, "a route the server stopped advertising is withdrawn"
  assert [route for route in renewed.routes if route.destination == "10.98.0.0"].len() == 1, "the foreign route survives"
}
