use tests.support.dhcp_pcap as pcap
use tests.support.dhcp_scenario as scenario

type Seen = {kind: Int, xid: Int, address: Str, server: Str, lease: Int, renew: Int?, rebind: Int?, mask: Str, routers: List[Str], dns: List[Str], domain: Str?, broadcast: Bool}

type ProbeResult = {offer: Seen, ack: Seen}

pure dnsmasq_flags() -> List[Str] {
  ["--dhcp-range=192.168.50.100,192.168.50.110,255.255.255.0,2m", "--dhcp-option=option:router,192.168.50.1", "--dhcp-option=option:dns-server,192.168.50.1,192.168.50.2", "--dhcp-option=option:domain-name,lab.example", "--dhcp-authoritative"]
}

proc require_tools() [process, error] -> Result[Bool, Error] {
  for tool in ["dnsmasq", "tcpdump"] {
    if process.which(tool) is Err(_) {
      test.skip(f"{tool} is not installed; the reference server runs in the Linux test image")
      return Ok(false)
    }
  }
  Ok(true)
}

test test_dnsmasq_offers_and_acknowledges_the_clients_request_and_both_captures_agree { |ctx|
  guard require_tools() else { return }
  guard let dir = scenario.execute(
    ctx,
    "reference",
    [
      scenario.tcpdump("server", "tcpdump.pcap"),
      scenario.capture("server", "recorder.pcap"),
      scenario.dnsmasq(dnsmasq_flags()),
      scenario.wait_for("dnsmasq.log", "DHCP, IP range", 10000),
      scenario.wait_for("recorder.pcap", "", 5000),
      scenario.probe(7001),
      scenario.sleep(500),
    ],
  )? else { return }
  assert fp"{dir}/probe.status".read_text()?.trim() == "0", fp"{dir}/probe.log".read_text()? + fp"{dir}/dnsmasq.log".read_text()?
  let result = json.read(fp"{dir}/probe.json")?.require(ProbeResult)?
  assert result.offer.kind == 2 and result.ack.kind == 5
  assert result.ack.address in ["192.168.50.100", "192.168.50.101", "192.168.50.102", "192.168.50.103", "192.168.50.104", "192.168.50.105", "192.168.50.106", "192.168.50.107", "192.168.50.108", "192.168.50.109", "192.168.50.110"]
  assert result.ack.server == "192.168.50.1" and result.ack.mask == "255.255.255.0"
  assert result.ack.routers == ["192.168.50.1"] and result.ack.dns == ["192.168.50.1", "192.168.50.2"] and result.ack.domain == "lab.example"
  assert result.ack.lease == 120 and result.ack.renew == 60 and result.ack.rebind == 105

  let ours = pcap.dhcp_messages(pcap.parse(fp"{dir}/recorder.pcap".read_bytes()?)?)
  let theirs = pcap.dhcp_messages(pcap.parse(fp"{dir}/tcpdump.pcap".read_bytes()?)?)
  assert [item.message.kind for item in ours] == [1, 2, 3, 5]
  assert [item.message.kind for item in theirs] == [1, 2, 3, 5]
  for index in range(4) {
    assert ours[index].message == theirs[index].message, f"frame {index} differs between the two recorders"
    assert ours[index].frame.udp?.payload == theirs[index].frame.udp?.payload
  }
  assert ours[1].frame.ip?.source == 3232248321 and ours[1].frame.ip?.checksum_ok == true
}
