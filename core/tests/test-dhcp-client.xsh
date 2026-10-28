use core.lib.dhcp_client as client
use core.lib.dhcp_machine as machine
use core.lib.dhcp_net as network
use core.lib.dhcp_store as store
use core.lib.dhcp_wire as wire
use core.tests.support.dhcp_fake as fake
use core.tests.support.dhcp_script as script
use core.tests.support.dhcp_types as types

const MAC = b"\x02\x00\x00\x00\x00\x01"
const SERVER = 3232248321
const OFFERED = 3232248420
const BROADCAST = 4294967295
const BOOT = "boot-one"

type Harness = {settings: client.Settings, link: client.Link, context: client.Context, root: Path}

type Outcome = {session: client.Session, world: fake.World}

pure fast() -> machine.Timing {
  machine.Timing(
    backoff_initial_ms: 40,
    backoff_max_ms: 640,
    jitter_ms: 10,
    min_retry_ms: 100,
    decline_delay_ms: 100,
    nak_delay_ms: 40,
    request_attempts: 3,
    reboot_attempts: 3,
    probe_count: 3,
    probe_min_ms: 10,
    probe_max_ms: 20,
    announce_wait_ms: 20,
    announce_interval_ms: 20,
    announce_count: 2,
    min_lease_secs: 2,
  )
}

proc harness(ctx: TestContext, name: Str) [fs, process, error] -> Result[Harness, Error] {
  let root = test.temp_dir(ctx, name:)?
  for child in ["lease", "run", "etc", "scratch"] {
    fp"{root}/{child}".mkdir()
  }
  let settings = client.Settings(
    interface: "eth0",
    lease_dir: fp"{root}/lease",
    run_dir: fp"{root}/run",
    resolv_dir: fp"{root}/etc",
    resolv_name: "resolv.conf",
    static_path: null,
    metric: 100,
    timing: fast(),
    scratch: fp"{root}/scratch",
  )
  let identity = wire.identity(MAC)?
  let link = client.Link(name: "eth0", ifindex: 3, mac: MAC, identity:)
  let sockets = client.Sockets(udp: -1, packets: -1, arp: -1, events: -1, control: -1)
  {settings: settings, link: link, context: client.Context(settings:, link:, sockets:, boot_id: BOOT), root: root}
}

pure ack_with(lease: Int) -> types.Reply {
  {...script.reply("ack"), lease: lease}
}

pure offer_with(lease: Int) -> types.Reply {
  {...script.reply("offer"), lease: lease}
}

pure after(at: Int, kind: Str, text: Str) -> fake.Happening {
  {at: at, kind: kind, text: text}
}

proc play(h: Harness, rules: List[types.Rule], happenings: List[fake.Happening]) [fs, process, net, time, error] -> Result[Outcome, Error] {
  let world = fake.world_of(h.link, 1000, rules, happenings)
  fake.save(h.settings, world)
  let record = store.load(fs.open_root(h.settings.lease_dir)?, "eth0")?
  let beginning = client.Beginning(record:, observed: fake.observed_in(world), admin_down: false)
  let session = client.drive(fake.scripted_io(), h.context, beginning, .Supervised)?
  {session: session, world: fake.load(h.settings)?}
}

pure sent_kinds(world: fake.World) -> List[Int] {
  [item.message for item in world.sent if item.kind == "dhcp"]
}

test test_a_client_acquires_probes_configures_publishes_and_records_the_lease { |ctx|
  let h = harness(ctx, "acquire")?
  let rules = [script.always("discover", [offer_with(600)]), script.always("request", [ack_with(600)])]
  let result = play(h, rules, [after(5000, "ask", "stop")])?
  assert sent_kinds(result.world)[..2] == [1, 3]
  let probes = [item for item in result.world.sent if item.kind == "arp" and item.destination == OFFERED]
  assert probes.len() >= 3
  let kernel = result.world.kernel
  assert kernel.addresses.len() == 1 and kernel.addresses[0].address == OFFERED
  assert kernel.routes.len() == 2
  let resolv = fp"{h.root}/etc/resolv.conf".read_text()?
  assert resolv.starts_with("# Managed by dhcpc") and "nameserver 192.168.50.1" in resolv
  guard let saved = store.load(fs.open_root(h.settings.lease_dir)?, "eth0")? else {
    assert false, "no lease record was written"
    return
  }
  assert saved.lease?.address == OFFERED and saved.owned.addresses.len() == 1
  assert "\"state\":\"bound\"" in fp"{h.root}/run/eth0.status".read_text()?
}

pure destinations(world: fake.World, message: Int) -> List[Int] {
  [item.destination for item in world.sent if item.kind == "dhcp" and item.message == message]
}

pure counted(session: client.Session, key: Str) -> Int {
  session.counters.get(key) ?? 0
}

test test_renewal_is_unicast_at_t1_then_rebinding_is_broadcast_at_t2_then_the_lease_expires { |ctx|
  let h = harness(ctx, "lifecycle")?
  let rules = [
    script.always("discover", [offer_with(20)]),
    script.only("request", 1, [ack_with(20)]),
    script.from("request", 2, []),
  ]
  let result = play(h, rules, [after(40000, "ask", "stop")])?
  let requests = destinations(result.world, 3)
  assert requests[0] == BROADCAST, "the selecting request is broadcast"
  assert SERVER in requests, "renewal goes to the server"
  assert requests.len() > 3
  assert requests[1] == SERVER, "the first renewal is the second request"
  let rebinding = [index for index in range(requests.len()) if index > 1 and requests[index] == BROADCAST]
  assert ! rebinding.is_empty(), "rebinding is broadcast after T2"
  assert counted(result.session, "lease_expired") >= 1
  let discovers = destinations(result.world, 1)
  assert discovers.len() >= 2, "expiry starts discovery again"
  assert result.world.kernel.addresses.is_empty() and result.world.kernel.routes.is_empty(), "expiry withdraws the owned configuration"
  assert fp"{h.root}/etc/resolv.conf".read_text()? == "# Managed by dhcpc: edit the static file, not this one.\n"
}

test test_an_acknowledged_renewal_with_new_options_reconfigures_routes_and_dns { |ctx|
  let h = harness(ctx, "renew-options")?
  let changed = {...ack_with(20), routers: ["192.168.50.2"], dns: ["192.168.50.53"]}
  let rules = [
    script.always("discover", [offer_with(20)]),
    script.only("request", 1, [ack_with(20)]),
    script.only("request", 2, [changed]),
    script.from("request", 3, [changed]),
  ]
  let result = play(h, rules, [after(18000, "ask", "stop")])?
  assert counted(result.session, "options_changed") == 1
  let gateways = [route.gateway for route in result.world.kernel.routes if route.protocol == 16]
  assert gateways == [3232248322]
  let resolv = fp"{h.root}/etc/resolv.conf".read_text()?
  assert "nameserver 192.168.50.53" in resolv and "192.168.50.1\n" not in resolv
  assert result.world.kernel.addresses.len() == 1
}

test test_a_nak_withdraws_only_owned_state_and_discovery_resumes { |ctx|
  let h = harness(ctx, "nak")?
  let rules = [
    script.always("discover", [offer_with(20)]),
    script.only("request", 1, [ack_with(20)]),
    script.only("request", 2, [script.reply("nak")]),
    script.from("request", 3, [ack_with(20)]),
  ]
  let result = play(h, rules, [after(25000, "ask", "stop")])?
  assert counted(result.session, "nak") >= 1
  assert destinations(result.world, 1).len() >= 2
  assert result.world.kernel.addresses.len() == 1, "the rediscovered lease is configured again"
}

test test_duplicate_out_of_order_unrelated_and_malformed_replies_never_stop_the_client { |ctx|
  let h = harness(ctx, "hostile")?
  let hostile_offer = [
    {...offer_with(600), xid: "other"},
    {...offer_with(600), chaddr: "other"},
    {...offer_with(600), corrupt: "truncate"},
    {...offer_with(600), corrupt: "cookie"},
    {...offer_with(600), corrupt: "option_length"},
    {...offer_with(600), corrupt: "no_end"},
    {...offer_with(600), corrupt: "garbage"},
    {...script.reply("ack"), lease: 600},
    offer_with(600),
    {...offer_with(600), repeat: 2},
  ]
  let hostile_ack = [
    {...ack_with(600), xid: "other"},
    {...script.reply("nak"), xid: "other"},
    {...ack_with(600), chaddr: "other"},
    ack_with(600),
    ack_with(600),
    script.reply("nak"),
  ]
  let rules = [script.always("discover", hostile_offer), script.always("request", hostile_ack)]
  let result = play(h, rules, [after(5000, "ask", "stop")])?
  assert result.world.kernel.addresses.len() == 1
  assert counted(result.session, "reject_xid_mismatch") >= 2
  assert counted(result.session, "reject_chaddr_mismatch") >= 2
  assert counted(result.session, "reject_too_short") + counted(result.session, "reject_bad_cookie") + counted(result.session, "reject_bad_option_length") + counted(result.session, "reject_missing_end") >= 3
  assert counted(result.session, "ignored_ack_in_selecting") >= 1 or counted(result.session, "ignored_stale_xid") >= 1
  assert machine.state_name(result.session.machine.state) == "bound"
}

test test_server_absence_backs_off_to_the_cap_and_the_client_keeps_trying { |ctx|
  let h = harness(ctx, "absent")?
  let result = play(h, [], [after(30000, "ask", "stop")])?
  let times = [item.at for item in result.world.sent if item.kind == "dhcp"]
  assert times.len() > 10
  let gaps = [times[index] - times[index - 1] for index in range(1, times.len())]
  assert gaps[0] >= 30 and gaps[0] <= 50
  assert gaps[1] >= 70 and gaps[1] <= 90
  assert gaps[2] >= 150 and gaps[2] <= 170
  assert gaps[3] >= 310 and gaps[3] <= 330
  for gap in gaps[4..] {
    assert gap >= 630 and gap <= 650, "the cap holds at 640 ms plus or minus the jitter"
  }
  assert result.world.kernel.addresses.is_empty()
}

test test_a_wall_clock_step_does_not_move_a_lease_deadline_and_a_boot_clock_jump_does { |ctx|
  let steady = harness(ctx, "steady")?
  let rules = [script.always("discover", [offer_with(20)]), script.always("request", [ack_with(20)])]
  let baseline = play(steady, rules, [after(15000, "ask", "stop")])?
  let stepped = harness(ctx, "wall-step")?
  let with_step = play(stepped, rules, [after(3000, "wall", "-86400000"), after(5000, "wall", "3600000000"), after(15000, "ask", "stop")])?
  assert [item.message for item in baseline.world.sent if item.kind == "dhcp"] == [item.message for item in with_step.world.sent if item.kind == "dhcp"]
  assert [item.at for item in baseline.world.sent] == [item.at for item in with_step.world.sent], "wall-clock steps leave every deadline where it was"

  let suspended = harness(ctx, "suspend")?
  let quiet = [script.always("discover", [offer_with(20)]), script.only("request", 1, [ack_with(20)]), script.from("request", 2, [])]
  let after_suspend = play(suspended, quiet, [after(3000, "jump", "60000"), after(70000, "ask", "stop")])?
  assert counted(after_suspend.session, "lease_expired") >= 1, "time spent suspended counts against the lease"
}

test test_a_boot_clock_that_goes_backwards_keeps_the_lease_and_revalidates_it { |ctx|
  let h = harness(ctx, "regression")?
  let rules = [script.always("discover", [offer_with(600)]), script.always("request", [ack_with(600)])]
  let result = play(h, rules, [after(5000, "jump", "-4500"), after(5000, "later", "100|ask|up"), after(12000, "ask", "stop")])?
  assert counted(result.session, "clock_regression") >= 1
  assert result.world.kernel.addresses.len() == 1
  assert 3 in sent_kinds(result.world)
}

test test_carrier_loss_and_return_revalidate_and_a_recreated_interface_reacquires { |ctx|
  let h = harness(ctx, "flap")?
  let rules = [script.always("discover", [offer_with(600)]), script.always("request", [ack_with(600)])]
  let flap = play(h, rules, [after(5000, "carrier", "down"), after(6000, "carrier", "up"), after(9000, "ask", "stop")])?
  assert counted(flap.session, "sent_request_init_reboot") >= 1
  assert flap.world.kernel.addresses.len() == 1

  let h2 = harness(ctx, "recreate")?
  let recreated = play(h2, rules, [after(5000, "recreate", ""), after(9000, "ask", "stop")])?
  assert recreated.world.kernel.ifindex == 13
  assert recreated.world.kernel.addresses.len() == 1, "the lease is revalidated and configured on the new device"
  assert counted(recreated.session, "sent_request_init_reboot") >= 1
}

test test_an_address_removed_behind_the_clients_back_with_a_lost_notification_is_repaired { |ctx|
  let h = harness(ctx, "repair")?
  let rules = [script.always("discover", [offer_with(600)]), script.always("request", [ack_with(600)])]
  let result = play(h, rules, [after(5000, "unplug_address", ""), after(8000, "ask", "stop")])?
  assert counted(result.session, "events_lost") == 1
  assert counted(result.session, "repaired") == 1
  assert result.world.kernel.addresses.len() == 1
}

test test_a_failed_network_operation_withdraws_and_a_later_attempt_converges { |ctx|
  let h = harness(ctx, "partial")?
  let rules = [script.always("discover", [offer_with(600)]), script.always("request", [ack_with(600)])]
  let result = play(h, rules, [after(1001, "fail_next", ""), after(9000, "ask", "stop")])?
  assert counted(result.session, "configure_failed") >= 1
  assert result.world.kernel.addresses.len() == 1, "the retry converged"
  assert machine.state_name(result.session.machine.state) == "bound"
}

test test_an_address_conflict_is_declined_and_another_offer_is_taken { |ctx|
  let h = harness(ctx, "conflict")?
  let second = {...offer_with(600), address: "192.168.50.101"}
  let second_ack = {...ack_with(600), address: "192.168.50.101"}
  let rules = [
    script.only("discover", 1, [offer_with(600)]),
    script.from("discover", 2, [second]),
    script.only("request", 1, [ack_with(600)]),
    script.from("request", 2, [second_ack]),
  ]
  let world = fake.world_of(h.link, 1000, rules, [after(8000, "ask", "stop")])
  fake.save(h.settings, {...world, conflicting: OFFERED})
  let beginning = client.Beginning(record: null, observed: fake.observed_in(world), admin_down: false)
  let session = client.drive(fake.scripted_io(), h.context, beginning, .Supervised)?
  let end = fake.load(h.settings)?
  assert 4 in sent_kinds(end), "the conflicting address is declined"
  assert end.kernel.addresses.len() == 1 and end.kernel.addresses[0].address == OFFERED + 1
  assert counted(session, "arp_conflict") == 1
}

test test_a_foreign_resolver_file_is_reported_and_never_replaced { |ctx|
  let h = harness(ctx, "resolver")?
  fp"{h.root}/etc/resolv.conf".write("# resolv.conf(5) file generated by tailscale\nnameserver 100.100.100.100\n")
  let rules = [script.always("discover", [offer_with(600)]), script.always("request", [ack_with(600)])]
  let result = play(h, rules, [after(5000, "ask", "stop")])?
  assert result.session.resolver == "dns-conflict foreign_file"
  assert fp"{h.root}/etc/resolv.conf".read_text()? == "# resolv.conf(5) file generated by tailscale\nnameserver 100.100.100.100\n"
  assert result.world.kernel.addresses.len() == 1, "a resolver conflict does not undo the address"
}
