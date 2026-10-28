use tests.support.dhcp_pcap as pcap
use tests.support.dhcp_scenario as scenario
use tests.support.dhcp_script as script
use tests.support.dhcp_types as types

type Seen = {kind: Int, xid: Int, address: Str, server: Str, lease: Int, renew: Int?, rebind: Int?, mask: Str, routers: List[Str], classless: List[Str], dns: List[Str], domain: Str?, broadcast: Bool}

type ProbeResult = {offer: Seen, ack: Seen}

const SERVER = 3232248321
const OFFERED = 3232248420

proc write_script(ctx: TestContext, rules: List[types.Rule]) [fs, process, error] -> Result[Path, Error] {
  let file = test.temp_file(ctx, name: "server-script.json", contents: bytes.from_text(json.encode({rules: rules})?))?
  file
}

test test_a_plain_client_gets_an_address_from_the_scripted_server_and_the_capture_agrees { |ctx|
  let rules = [script.always("discover", [script.reply("offer")]), script.always("request", [script.reply("ack")])]
  let file = write_script(ctx, rules)?
  guard let dir = scenario.execute(
    ctx,
    "dora",
    [
      scenario.capture("server", "server.pcap"),
      scenario.server_xsh(file),
      scenario.wait_for("server.pcap", "", 5000),
      scenario.wait_for("server.requests", "", 5000),
      scenario.probe(4242),
      scenario.sleep(200),
    ],
  )? else { return }
  assert fp"{dir}/probe.status".read_text()?.trim() == "0", fp"{dir}/probe.log".read_text()?
  let result = json.read(fp"{dir}/probe.json")?.require(ProbeResult)?
  assert result.offer.address == "192.168.50.100" and result.ack.lease == 600
  let exchanges = pcap.dhcp_messages(pcap.parse(fp"{dir}/server.pcap".read_bytes()?)?)
  let kinds = [item.message.kind for item in exchanges]
  assert kinds == [1, 2, 3, 5]
  let discover = exchanges[0]
  assert discover.frame.destination_mac == b"\xff\xff\xff\xff\xff\xff" and discover.frame.ip?.destination == 4294967295
  assert discover.frame.udp?.source_port == 68 and discover.frame.udp?.destination_port == 67
  assert discover.message.broadcast and discover.message.ciaddr == 0 and discover.message.xid == 4242
  let request = exchanges[2]
  assert request.message.requested_ip == OFFERED and request.message.server_id == SERVER
  assert exchanges[1].frame.ip?.source == SERVER
}

type SnapshotLink = {name: Str?, ifindex: Int, flags: Int}

type Snapshot = {links: List[SnapshotLink]}

test test_the_veth_peer_is_created_with_the_requested_name_in_the_server_namespace { |ctx|
  guard let dir = scenario.execute(ctx, "veth", [scenario.snapshot("client")])? else { return }
  let names = [link.name ?? "" for link in json.read(fp"{dir}/client.json")?.require(Snapshot)?.links]
  assert "eth0" in names and "lo" in names
  for name in names {
    assert name != "srv0" and ! name.starts_with("veth"), f"the peer must live in the server namespace, found {name}"
  }
  assert fp"{dir}/server-link.log".read_text()? == ""
}
