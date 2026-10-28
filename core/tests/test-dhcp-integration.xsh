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
