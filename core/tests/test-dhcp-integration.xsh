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
