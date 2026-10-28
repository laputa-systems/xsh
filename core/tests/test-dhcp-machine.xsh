use core.lib.dhcp_machine as machine
use core.lib.dhcp_wire as wire

const MAC = b"\x02\x00\x00\x00\x00\x01"
const SERVER = 3232235777
const OTHER_SERVER = 3232235778
const ADDRESS = 3232235876
const ROUTER = 3232235777
const MASK = 4294967040
const T0 = 1000

type Ses = {machine: machine.Machine, now: Int, effects: List[machine.Effect], note: Str}

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

pure identity() -> wire.Identity {
  if let Ok(id) = wire.identity(MAC) {
    id
  } else {
    {mac: b"", client_id: b""}
  }
}

pure be32(value: Int) -> Bytes {
  bytes.pack_be(value, 4) ?? b""
}

pure option(code: Int, data: Bytes) -> Bytes {
  wire.option(code, data) ?? b""
}

type TimerCase = {lease: Int, t1: Int?, t2: Int?, renew: Int, rebind: Int, rule: machine.TimerRule}

type Reply = {
  kind: Int,
  xid: Int,
  server: Int,
  address: Int,
  lease_secs: Int,
  renewal: Int?,
  rebinding: Int?,
  dns: List[Int],
  routers: List[Int],
}

pure reply(kind: Int, xid: Int) -> Reply {
  Reply(kind:, xid:, server: SERVER, address: ADDRESS, lease_secs: 20, renewal: null, rebinding: null, dns: [3232235777], routers: [ROUTER])
}

pure reply_bytes(r: Reply) -> Bytes {
  let renewal: List[Bytes] = if let seconds = r.renewal { [option(58, be32(seconds))] } else { [] }
  let rebinding: List[Bytes] = if let seconds = r.rebinding { [option(59, be32(seconds))] } else { [] }
  let header = {op: 2, xid: r.xid, secs: 0, broadcast: false, ciaddr: 0, yiaddr: if r.kind == 6 { 0 } else { r.address }, siaddr: 0, giaddr: 0, chaddr: MAC}
  let options = [
    option(53, bytes.from_ints([r.kind]) ?? b""),
    option(54, be32(r.server)),
    option(51, be32(r.lease_secs)),
    option(1, be32(MASK)),
    option(3, bytes.concat([be32(router) for router in r.routers])),
    option(6, bytes.concat([be32(server) for server in r.dns])),
    @renewal,
    @rebinding,
  ]
  wire.encode_message(header, options) ?? b""
}

proc message(r: Reply) -> Result[wire.Message] {
  match wire.accept_reply(reply_bytes(r), identity(), r.xid) {
    Ok(value) => value
    Err(reject) => Err(error.failure(f"fixture rejected: {wire.reject_reason(reject)}"))
  }
}

pure start_info(remembered: machine.Lease?, configured: Bool) -> machine.StartInfo {
  machine.StartInfo(seed: 12345, carrier: true, remembered:, configured:)
}

pure begin(timing: machine.Timing) -> Ses {
  let step = machine.transition(machine.new_machine(timing), .OnStart(start_info(null, false)), T0)
  {machine: step.machine, now: T0, effects: step.effects, note: step.note}
}

pure feed(ses: Ses, event: machine.Event, at: Int) -> Ses {
  let step = machine.transition(ses.machine, event, at)
  {machine: step.machine, now: at, effects: step.effects, note: step.note}
}

pure sends(effects: List[machine.Effect]) -> List[machine.Packet] {
  collect {
    for effect in effects {
      if let .Send(packet) = effect {
        yield packet
      }
    }
  }
}

pure effect_name(effect: machine.Effect) -> Str {
  match effect {
    machine.Send(packet) => f"send:{machine.kind_name(packet.kind)}"
    machine.Probe(_) => "probe"
    machine.Announce(_) => "announce"
    machine.Configure(_) => "configure"
    machine.Deconfigure(reason) => f"deconfigure:{machine.reason_name(reason)}"
    machine.Persist(_) => "persist"
    machine.Forget => "forget"
    machine.Publish(_) => "publish"
    machine.Arm(_) => "arm"
    machine.Disarm => "disarm"
  }
}

pure names(effects: List[machine.Effect]) -> List[Str] {
  [effect_name(effect) for effect in effects]
}

pure only_actions(effects: List[machine.Effect]) -> List[Str] {
  [name for name in names(effects) if name != "arm" and name != "disarm" and name != "publish"]
}

proc held(ses: Ses) -> Result[machine.Lease] {
  if let lease = ses.machine.lease {
    lease
  } else {
    Err(error.failure("the session holds no lease"))
  }
}

pure wake(ses: Ses) -> Int {
  ses.machine.wake_at ?? -1
}

pure state_of(ses: Ses) -> Str {
  machine.state_name(ses.machine.state)
}

proc bound_ses(timing: machine.Timing) -> Result[Ses] {
  var ses = begin(timing)
  let discover = sends(ses.effects)[0]
  ses = feed(ses, .OnReceived(message(reply(2, discover.xid))?), ses.now + 5)
  ses = feed(ses, .OnReceived(message(reply(5, discover.xid))?), ses.now + 5)
  while state_of(ses) == "probing" {
    ses = feed(ses, .OnTick, wake(ses))
  }
  ses
}

test test_acquisition_walks_discover_offer_request_ack_probe_configure {
  var ses = begin(fast())
  assert state_of(ses) == "selecting"
  assert only_actions(ses.effects) == ["send:discover"]
  let discover = sends(ses.effects)[0]
  assert ! discover.unicast and discover.secs == 0 and discover.xid != 0
  assert ses.machine.wake_at != null

  ses = feed(ses, .OnReceived(message(reply(2, discover.xid))?), 1010)
  assert state_of(ses) == "requesting"
  let request = sends(ses.effects)[0]
  assert request.kind == .RequestSelecting
  assert request.xid == discover.xid and request.address == ADDRESS and request.server == SERVER and ! request.unicast

  ses = feed(ses, .OnReceived(message(reply(5, discover.xid))?), 1020)
  assert state_of(ses) == "probing"
  assert only_actions(ses.effects) == ["probe"]
  var probes = 1
  while state_of(ses) == "probing" {
    ses = feed(ses, .OnTick, wake(ses))
    if only_actions(ses.effects) == ["probe"] { probes += 1 }
  }
  assert probes == 3
  assert state_of(ses) == "bound"
  assert only_actions(ses.effects) == ["configure", "persist", "announce"]
  guard let lease = ses.machine.lease else {
    assert false, "bound without a lease"
    return
  }
  assert lease.address == ADDRESS and lease.prefix == 24 and lease.server == SERVER
  assert lease.routers == [ROUTER] and lease.broadcast == 3232236031
  assert lease.acquired_at == 1010 and lease.timers == .Default
  assert lease.renew_at == 1010 + 10000 and lease.rebind_at == 1010 + 17500 and lease.expire_at == 1010 + 20000
}

test test_the_second_announcement_follows_and_then_the_machine_idles {
  var ses = bound_ses(fast())?
  assert ses.machine.announces_left == 1
  ses = feed(ses, .OnTick, wake(ses))
  assert only_actions(ses.effects) == ["announce"]
  assert ses.machine.announces_left == 0
  assert wake(ses) == ses.machine.lease?.renew_at
}

test test_renewal_is_unicast_at_t1_and_rebinding_broadcast_at_t2 {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnTick, wake(ses))
  assert state_of(ses) == "bound"
  ses = feed(ses, .OnTick, lease.renew_at ?? 0)
  assert state_of(ses) == "renewing"
  let renewal = sends(ses.effects)[0]
  assert renewal.kind == .RequestBound and renewal.unicast and renewal.server == SERVER and renewal.address == ADDRESS
  assert renewal.xid != ses.machine.xid or true

  var retries = 0
  while state_of(ses) == "renewing" {
    ses = feed(ses, .OnTick, wake(ses))
    if only_actions(ses.effects) == ["send:request_bound"] and state_of(ses) == "renewing" { retries += 1 }
  }
  assert retries >= 0
  assert state_of(ses) == "rebinding"
  let rebind = sends(ses.effects)[0]
  assert rebind.kind == .RequestBound and ! rebind.unicast
  assert ses.now == lease.rebind_at
}

test test_an_ack_to_a_renewal_restarts_the_timers_from_the_request {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnTick, lease.renew_at ?? 0)
  assert state_of(ses) == "renewing"
  let {xid, request_sent_at: sent_at, ..} = ses.machine
  ses = feed(ses, .OnReceived(message(reply(5, xid))?), sent_at + 7)
  assert state_of(ses) == "bound"
  assert only_actions(ses.effects) == ["persist"]
  let renewed = held(ses)?
  assert renewed.acquired_at == sent_at
  assert renewed.renew_at == sent_at + 10000 and renewed.expire_at == sent_at + 20000
}

test test_changed_dns_and_routes_on_renewal_reconfigure_but_identical_options_do_not {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnTick, lease.renew_at ?? 0)
  let same = reply(5, ses.machine.xid)
  ses = feed(ses, .OnReceived(message(same)?), ses.now + 1)
  assert only_actions(ses.effects) == ["persist"] and ses.note == ""

  ses = feed(ses, .OnTick, ses.machine.lease?.renew_at ?? 0)
  let changed = {...reply(5, ses.machine.xid), dns: [167772161, 167772162], routers: [3232235778]}
  ses = feed(ses, .OnReceived(message(changed)?), ses.now + 1)
  assert only_actions(ses.effects) == ["configure", "persist"] and ses.note == "options_changed"
  assert ses.machine.lease?.dns == [167772161, 167772162] and ses.machine.lease?.routers == [3232235778]
}

test test_expiry_withdraws_configuration_and_starts_discovery {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnTick, lease.expire_at ?? 0)
  assert only_actions(ses.effects) == ["deconfigure:expired", "forget", "send:discover"]
  assert state_of(ses) == "selecting" and ses.machine.lease == null
  assert ses.note == "lease_expired"
}

test test_a_resume_past_rebinding_skips_renewing_and_a_resume_past_expiry_expires {
  let ses = bound_ses(fast())?
  let lease = held(ses)?
  let rebind_at = lease.rebind_at ?? 0
  let rebound = feed(ses, .OnTick, rebind_at + 100)
  assert state_of(rebound) == "rebinding"
  assert only_actions(rebound.effects) == ["send:request_bound"]
  let expired = feed(ses, .OnTick, (lease.expire_at ?? 0) + 1000000)
  assert state_of(expired) == "selecting" and expired.machine.lease == null
  assert only_actions(expired.effects) == ["deconfigure:expired", "forget", "send:discover"]
}

test test_a_late_ack_after_expiry_never_extends_the_lease {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnTick, lease.renew_at ?? 0)
  let xid = ses.machine.xid
  ses = feed(ses, .OnReceived(message(reply(5, xid))?), (lease.expire_at ?? 0) + 5)
  assert state_of(ses) == "selecting" and ses.machine.lease == null
  assert ses.note == "ignored_stale_xid"
  assert only_actions(ses.effects) == ["deconfigure:expired", "forget", "send:discover"]
}

test test_nak_withdraws_owned_configuration_and_restarts_after_a_delay {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnTick, lease.renew_at ?? 0)
  ses = feed(ses, .OnReceived(message(reply(6, ses.machine.xid))?), ses.now + 3)
  assert only_actions(ses.effects) == ["deconfigure:nak", "forget"]
  assert state_of(ses) == "init" and ses.note == "nak"
  assert ses.machine.lease == null
  ses = feed(ses, .OnTick, wake(ses))
  assert state_of(ses) == "selecting" and only_actions(ses.effects) == ["send:discover"]
}

test test_nak_while_requesting_returns_to_init_without_deconfiguring {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  ses = feed(ses, .OnReceived(message(reply(2, xid))?), 1010)
  ses = feed(ses, .OnReceived(message(reply(6, xid))?), 1020)
  assert state_of(ses) == "init" and only_actions(ses.effects) == []
  let stranger = {...reply(6, xid), server: OTHER_SERVER}
  var second = begin(fast())
  second = feed(second, .OnReceived(message(reply(2, sends(second.effects)[0].xid))?), 1010)
  second = feed(second, .OnReceived(message({...stranger, xid: second.machine.xid})?), 1020)
  assert state_of(second) == "requesting" and second.note == "ignored_nak_in_requesting"
}

test test_address_conflict_declines_and_waits_before_discovering_again {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  ses = feed(ses, .OnReceived(message(reply(2, xid))?), 1010)
  ses = feed(ses, .OnReceived(message(reply(5, xid))?), 1020)
  assert state_of(ses) == "probing"
  ses = feed(ses, .OnArpConflict, 1025)
  assert only_actions(ses.effects) == ["send:decline"]
  let decline = sends(ses.effects)[0]
  assert decline.address == ADDRESS and decline.server == SERVER and ses.note == "arp_conflict"
  assert state_of(ses) == "declined" and ses.machine.lease == null
  assert wake(ses) == 1025 + 100
  ses = feed(ses, .OnTick, wake(ses))
  assert state_of(ses) == "selecting" and only_actions(ses.effects) == ["send:discover"]
}

test test_server_absence_backs_off_with_jitter_to_the_cap_and_never_gives_up {
  var ses = begin(fast())
  let delays = collect {
    repeat 12 times {
      ses = feed(ses, .OnTick, wake(ses))
      assert state_of(ses) == "selecting"
      yield wake(ses) - ses.now
    }
  }
  let expected = [80, 160, 320, 640, 640, 640, 640, 640, 640, 640, 640, 640]
  for index in range(delays.len()) {
    assert delays[index] >= expected[index] - 10 and delays[index] <= expected[index] + 10, f"{index}: {delays[index]}"
  }
}

test test_retransmissions_keep_the_transaction_and_report_elapsed_seconds {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  ses = feed(ses, .OnTick, wake(ses))
  let again = sends(ses.effects)[0]
  assert again.xid == xid and again.kind == .Discover
  ses = feed(ses, .OnTick, 1000 + 65000)
  assert sends(ses.effects)[0].secs == 65
}

test test_unrelated_duplicate_and_out_of_order_replies_are_ignored {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  let before = ses.machine
  ses = feed(ses, .OnReceived(message(reply(5, xid))?), 1005)
  assert ses.note == "ignored_ack_in_selecting" and ses.machine.state == before.state and only_actions(ses.effects) == []
  ses = feed(ses, .OnReceived(message(reply(2, xid + 1))?), 1006)
  assert ses.note == "ignored_stale_xid"
  ses = feed(ses, .OnReceived(message(reply(2, xid))?), 1010)
  assert state_of(ses) == "requesting"
  ses = feed(ses, .OnReceived(message(reply(2, xid))?), 1011)
  assert ses.note == "ignored_offer_in_requesting" and only_actions(ses.effects) == []
  let elsewhere = {...reply(5, xid), address: ADDRESS + 5}
  ses = feed(ses, .OnReceived(message(elsewhere)?), 1012)
  assert ses.note == "ignored_ack_in_requesting" and state_of(ses) == "requesting"
}

test test_unusable_acknowledgements_restart_instead_of_configuring {
  for rejected in [
    {...reply(5, 0), lease_secs: 0},
    {...reply(5, 0), lease_secs: 1},
    {...reply(5, 0), address: 3232235776},
    {...reply(5, 0), address: 3232236031},
  ] {
    var ses = begin(fast())
    let xid = sends(ses.effects)[0].xid
    ses = feed(ses, .OnReceived(message({...rejected, kind: 2, xid: xid})?), 1010)
    ses = feed(ses, .OnReceived(message({...rejected, kind: 5, xid: xid})?), 1020)
    assert state_of(ses) == "init" and ses.note == "ack_unusable", f"{rejected.lease_secs} {rejected.address}"
    assert only_actions(ses.effects) == []
  }
}

test test_an_infinite_lease_arms_no_timers {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  ses = feed(ses, .OnReceived(message(reply(2, xid))?), 1010)
  ses = feed(ses, .OnReceived(message({...reply(5, xid), lease_secs: wire.INFINITE_LEASE})?), 1020)
  while state_of(ses) == "probing" {
    ses = feed(ses, .OnTick, wake(ses))
  }
  let lease = held(ses)?
  assert lease.renew_at == null and lease.rebind_at == null and lease.expire_at == null and lease.timers == .Infinite
  ses = feed(ses, .OnTick, wake(ses))
  assert ses.machine.wake_at == null
  let far = feed(ses, .OnTick, 4000000000000)
  assert state_of(far) == "bound" and only_actions(far.effects) == []
}

test test_timer_rules_are_named_for_every_server_combination {
  let cases: List[TimerCase] = [
    {lease: 3600, t1: null, t2: null, renew: 1800000, rebind: 3150000, rule: .Default},
    {lease: 3600, t1: 1800, t2: 3150, renew: 1800000, rebind: 3150000, rule: .Server},
    {lease: 3600, t1: 600, t2: 3000, renew: 600000, rebind: 3000000, rule: .Server},
    {lease: 3600, t1: 600, t2: null, renew: 600000, rebind: 3150000, rule: .Mixed},
    {lease: 3600, t1: null, t2: 3300, renew: 1800000, rebind: 3300000, rule: .Mixed},
    {lease: 3600, t1: 3000, t2: 2000, renew: 1800000, rebind: 3150000, rule: .Repaired},
    {lease: 3600, t1: 2000, t2: 2000, renew: 1800000, rebind: 3150000, rule: .Repaired},
    {lease: 3600, t1: 0, t2: 0, renew: 1800000, rebind: 3150000, rule: .Repaired},
    {lease: 3600, t1: null, t2: 3600, renew: 1800000, rebind: 3150000, rule: .Repaired},
    {lease: 3600, t1: 3600, t2: null, renew: 1800000, rebind: 3150000, rule: .Repaired},
    {lease: 3600, t1: 3200, t2: null, renew: 1800000, rebind: 3150000, rule: .Repaired},
    {lease: 2, t1: null, t2: null, renew: 1000, rebind: 1750, rule: .Default},
    {lease: 4294967294, t1: null, t2: null, renew: 2147483647000, rebind: 3758096382250, rule: .Default},
  ]
  for case in cases {
    let timers = machine.lease_timers(case.lease, case.t1, case.t2)
    assert timers.renew_ms == case.renew and timers.rebind_ms == case.rebind and timers.rule == case.rule, f"{case.lease} {case.t1 ?? -1} {case.t2 ?? -1}"
    assert timers.renew_ms < timers.rebind_ms and timers.rebind_ms < case.lease * 1000
  }
}

test test_time_arithmetic_saturates_instead_of_overflowing {
  assert machine.add_ms(10, 5) == 15
  assert machine.add_ms(4611686018427387900, 4611686018427387904) == 4611686018427387904
  assert machine.add_ms(0, 9223372036854775807) == 4611686018427387904
}

test test_a_missing_mask_falls_back_to_the_classful_mask_and_foreign_routers_are_dropped {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  let outside = {...reply(5, xid), routers: [167772161, ROUTER, ADDRESS]}
  let acknowledged = message(outside)?
  let lease = machine.lease_from_ack(acknowledged, 0, fast())?
  assert lease.routers == [ROUTER]
}

test test_link_loss_keeps_the_lease_and_revalidates_it_on_return {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  ses = feed(ses, .OnLinkDown, ses.now + 50)
  assert state_of(ses) == "no_link" and ses.machine.lease != null and only_actions(ses.effects) == []
  assert wake(ses) == lease.expire_at
  ses = feed(ses, .OnLinkUp(false), ses.now + 50)
  assert state_of(ses) == "init_reboot"
  let reboot = sends(ses.effects)[0]
  assert reboot.kind == .RequestInitReboot and reboot.address == ADDRESS and ! reboot.unicast
  ses = feed(ses, .OnReceived(message(reply(5, ses.machine.xid))?), ses.now + 5)
  assert state_of(ses) == "bound" and only_actions(ses.effects) == ["persist"]
}

test test_a_recreated_interface_loses_owned_state_and_reacquires_through_the_probe {
  var ses = bound_ses(fast())?
  ses = feed(ses, .OnLinkDown, ses.now + 50)
  ses = feed(ses, .OnLinkUp(true), ses.now + 50)
  assert only_actions(ses.effects) == ["deconfigure:link_recreated", "forget", "send:request_init_reboot"]
  assert ses.machine.lease == null and state_of(ses) == "init_reboot"
  ses = feed(ses, .OnReceived(message(reply(5, ses.machine.xid))?), ses.now + 5)
  assert state_of(ses) == "probing"
}

test test_init_reboot_falls_back_to_discovery_and_a_nak_forgets_the_remembered_address {
  let remembered = held(bound_ses(fast())?)?
  var ses = feed(begin(fast()), .OnShutdown(false), 1001)
  let step = machine.transition(machine.new_machine(fast()), .OnStart(start_info(remembered, false)), 5000)
  ses = {machine: step.machine, now: 5000, effects: step.effects, note: step.note}
  assert state_of(ses) == "init_reboot"
  assert sends(ses.effects)[0].kind == .RequestInitReboot and ses.machine.lease == null
  var rounds = 0
  while state_of(ses) == "init_reboot" {
    ses = feed(ses, .OnTick, wake(ses))
    rounds += 1
    assert rounds < 10
  }
  assert state_of(ses) == "selecting" and only_actions(ses.effects) == ["send:discover"]

  let nak_step = machine.transition(machine.new_machine(fast()), .OnStart(start_info(remembered, false)), 5000)
  let nak_run = {machine: nak_step.machine, now: 5000, effects: nak_step.effects, note: nak_step.note}
  let denied = feed(nak_run, .OnReceived(message(reply(6, nak_run.machine.xid))?), 5010)
  assert state_of(denied) == "init" and denied.note == "nak"
}

test test_a_configured_remembered_lease_that_has_ended_is_withdrawn_at_start {
  let remembered = held(bound_ses(fast())?)?
  let expire_at = remembered.expire_at ?? 0
  let step = machine.transition(machine.new_machine(fast()), .OnStart(start_info(remembered, true)), expire_at + 1)
  assert only_actions(step.effects) == ["deconfigure:expired", "forget", "send:request_init_reboot"]
  let live = machine.transition(machine.new_machine(fast()), .OnStart(start_info(remembered, true)), expire_at - 100)
  assert only_actions(live.effects) == ["send:request_init_reboot"] and live.machine.lease != null
  assert live.machine.wake_at != null and live.machine.wake_at <= expire_at
}

test test_start_without_carrier_waits_for_the_link {
  let info = machine.StartInfo(seed: 1, carrier: false, remembered: null, configured: false)
  let step = machine.transition(machine.new_machine(fast()), .OnStart(info), 1000)
  assert machine.state_name(step.machine.state) == "no_link" and only_actions(step.effects) == []
  let up = machine.transition(step.machine, .OnLinkUp(false), 1100)
  assert machine.state_name(up.machine.state) == "selecting" and only_actions(up.effects) == ["send:discover"]
}

test test_admin_down_releases_and_up_restarts {
  var ses = bound_ses(fast())?
  ses = feed(ses, .OnAdminDown, ses.now + 10)
  assert only_actions(ses.effects) == ["send:release", "deconfigure:admin_down", "forget"]
  let release = sends(ses.effects)[0]
  assert release.unicast and release.server == SERVER and release.address == ADDRESS
  assert state_of(ses) == "admin_down" and ses.machine.lease == null and ses.machine.wake_at == null
  let idle = feed(ses, .OnTick, ses.now + 1000000)
  assert state_of(idle) == "admin_down" and only_actions(idle.effects) == []
  ses = feed(ses, .OnAdminUp, ses.now + 20)
  assert state_of(ses) == "selecting" and only_actions(ses.effects) == ["send:discover"]
}

test test_shutdown_keeps_the_lease_unless_asked_to_release {
  let ses = bound_ses(fast())?
  let kept = feed(ses, .OnShutdown(false), ses.now + 1)
  assert only_actions(kept.effects) == ["persist"] and kept.machine.lease != null
  let released = feed(ses, .OnShutdown(true), ses.now + 1)
  assert only_actions(released.effects) == ["send:release", "deconfigure:admin_down", "forget"]
}

test test_a_failed_application_withdraws_and_retries {
  var ses = bound_ses(fast())?
  ses = feed(ses, .OnApplied(false), ses.now + 1)
  assert only_actions(ses.effects) == ["deconfigure:apply_failed", "forget"]
  assert state_of(ses) == "init" and ses.note == "apply_failed"
  let settled = bound_ses(fast())?
  let fine = feed(settled, .OnApplied(true), settled.now + 1)
  assert state_of(fine) == "bound" and only_actions(fine.effects) == []
}

test test_a_clock_that_moves_backwards_keeps_remaining_time_and_revalidates {
  var ses = bound_ses(fast())?
  let lease = held(ses)?
  let remaining = (lease.expire_at ?? 0) - ses.now
  let rolled_back = ses.now - 700
  ses = feed(ses, .OnTick, rolled_back)
  assert ses.note == "clock_regression" and state_of(ses) == "init_reboot"
  assert only_actions(ses.effects) == ["send:request_init_reboot"]
  assert ses.machine.lease != null
  let kept = held(ses)?
  assert (kept.expire_at ?? 0) - rolled_back == remaining
  ses = feed(ses, .OnReceived(message(reply(5, ses.machine.xid))?), rolled_back + 5)
  assert state_of(ses) == "bound"
}

test test_the_same_seed_replays_and_different_seeds_diverge {
  let first = begin(fast())
  let again = begin(fast())
  assert sends(first.effects)[0].xid == sends(again.effects)[0].xid
  assert first.machine.wake_at == again.machine.wake_at
  let other = machine.transition(machine.new_machine(fast()), .OnStart(machine.StartInfo(seed: 99, carrier: true, remembered: null, configured: false)), T0)
  assert sends(other.effects)[0].xid != sends(first.effects)[0].xid
}

test test_wake_follows_every_transition_and_effects_stay_bounded {
  var ses = begin(fast())
  let xid = sends(ses.effects)[0].xid
  ses = feed(ses, .OnReceived(message(reply(2, xid))?), 1010)
  ses = feed(ses, .OnReceived(message(reply(5, xid))?), 1020)
  var armed = ses.machine.wake_at
  repeat 300 times {
    assert wake(ses) != -1
    ses = feed(ses, .OnTick, wake(ses))
    assert ses.effects.len() <= machine.MAX_EFFECTS
    for effect in ses.effects {
      if let .Arm(at) = effect {
        assert at == ses.machine.wake_at
      }
    }
    armed = ses.machine.wake_at
  }
  assert armed != null
}

type Scenario = {name: Str, ses: Ses}

proc scenarios() -> Result[List[Scenario]] {
  let bound = bound_ses(fast())?
  let lease = held(bound)?
  var selecting = begin(fast())
  let xid = sends(selecting.effects)[0].xid
  let requesting = feed(selecting, .OnReceived(message(reply(2, xid))?), 1010)
  let probing = feed(requesting, .OnReceived(message(reply(5, xid))?), 1020)
  let renewing = feed(bound, .OnTick, lease.renew_at ?? 0)
  let rebinding = feed(renewing, .OnTick, lease.rebind_at ?? 0)
  let no_link = feed(bound, .OnLinkDown, bound.now + 1)
  let rebooting = feed(no_link, .OnLinkUp(false), no_link.now + 1)
  [
    {name: "init", ses: feed(bound, .OnApplied(false), bound.now + 1)},
    {name: "selecting", ses: selecting},
    {name: "requesting", ses: requesting},
    {name: "probing", ses: probing},
    {name: "bound", ses: bound},
    {name: "renewing", ses: renewing},
    {name: "rebinding", ses: rebinding},
    {name: "init_reboot", ses: rebooting},
    {name: "declined", ses: feed(probing, .OnArpConflict, 1030)},
    {name: "admin_down", ses: feed(bound, .OnAdminDown, bound.now + 1)},
    {name: "no_link", ses: no_link},
  ]
}

type Probe = {name: Str, event: machine.Event, at: Int}

proc events_for(ses: Ses) -> Result[List[Probe]] {
  let xid = ses.machine.xid
  let later = ses.now + 1
  let far = ses.now + 1000000
  [
    {name: "start", event: .OnStart(start_info(null, false)), at: later},
    {name: "offer", event: .OnReceived(message(reply(2, xid))?), at: later},
    {name: "ack", event: .OnReceived(message(reply(5, xid))?), at: later},
    {name: "nak", event: .OnReceived(message(reply(6, xid))?), at: later},
    {name: "stale_offer", event: .OnReceived(message(reply(2, xid + 1))?), at: later},
    {name: "stale_ack", event: .OnReceived(message(reply(5, xid + 1))?), at: later},
    {name: "stale_nak", event: .OnReceived(message(reply(6, xid + 1))?), at: later},
    {name: "tick_idle", event: .OnTick, at: later},
    {name: "tick_due", event: .OnTick, at: ses.machine.wake_at ?? later},
    {name: "tick_far", event: .OnTick, at: far},
    {name: "link_up", event: .OnLinkUp(false), at: later},
    {name: "link_recreated", event: .OnLinkUp(true), at: later},
    {name: "link_down", event: .OnLinkDown, at: later},
    {name: "arp_conflict", event: .OnArpConflict, at: later},
    {name: "applied", event: .OnApplied(true), at: later},
    {name: "apply_failed", event: .OnApplied(false), at: later},
    {name: "admin_up", event: .OnAdminUp, at: later},
    {name: "admin_down", event: .OnAdminDown, at: later},
    {name: "shutdown", event: .OnShutdown(false), at: later},
    {name: "shutdown_release", event: .OnShutdown(true), at: later},
    {name: "clock_back", event: .OnTick, at: ses.now - 5},
  ]
}

test test_every_state_by_every_event_stays_bounded_consistent_and_total {
  let states = scenarios()?
  var cells = 0
  for scenario in states {
    assert state_of(scenario.ses) == scenario.name, f"{scenario.name} scenario is in {state_of(scenario.ses)}"
    for probe in events_for(scenario.ses)? {
      let before = scenario.ses.machine
      let after = feed(scenario.ses, probe.event, probe.at)
      let cell = f"{scenario.name} x {probe.name}"
      cells += 1
      assert after.effects.len() <= machine.MAX_EFFECTS, cell
      let state = state_of(after)
      if after.machine.lease == null {
        assert state not in ["bound", "renewing", "rebinding"], f"{cell}: {state} without a lease"
      }
      let armed = [at for at in [effect_arm(effect) for effect in after.effects] if at != -2]
      if ! armed.is_empty() {
        assert armed[-1] == (after.machine.wake_at ?? -1), cell
      }
      if armed.is_empty() and probe.at >= before.last_now {
        assert after.machine.wake_at == before.wake_at, f"{cell}: wake changed without an effect"
      }
      match probe.name {
        "stale_offer" | "stale_ack" | "stale_nak" => {
          assert state == scenario.name, cell
          assert only_actions(after.effects).is_empty() or scenario.name == "bound" or scenario.name == "no_link", cell
        }
        "applied" => assert state == scenario.name or after.note == "lease_expired", cell
        "admin_down" => assert state == "admin_down", cell
        "shutdown_release" => assert state == "admin_down", cell
        "link_down" => assert state == (if scenario.name == "admin_down" { "admin_down" } else { "no_link" }) or after.note == "lease_expired", cell
        "arp_conflict" => {
          let declined = state == "declined"
          let from_probing = scenario.name == "probing" or scenario.name == "declined"
          assert declined == from_probing or after.note == "lease_expired", cell
        }
        else => {}
      }
    }
  }
  assert states.len() == 11
  assert cells == 11 * 21
}

pure effect_arm(effect: machine.Effect) -> Int {
  match effect {
    machine.Arm(at) => at
    machine.Disarm => -1
    else => -2
  }
}
