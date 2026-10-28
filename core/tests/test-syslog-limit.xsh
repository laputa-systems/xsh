use core.lib.syslog_limit as limit

const SECOND = 1000000
const TIGHT = limit.Config(sender_rate: 10, sender_burst: 5, global_rate: 1000, global_burst: 1000)

proc plain(uid: Int, pid: Int) -> limit.Datagram {
  limit.Datagram(sender: limit.Sender(uid: uid, pid: pid), oversize: false, control_truncated: false, rights: 0)
}

proc dropped_total(l: limit.Limiter) -> Int {
  let c = l.counters
  c.rate + c.oversize + c.control_truncated + c.rights + c.sink_blocked + c.sink_error
}

test test_configuration_bounds_are_enforced {
  for config in [
    limit.Config(sender_rate: 0, sender_burst: 5, global_rate: 10, global_burst: 10),
    limit.Config(sender_rate: 5, sender_burst: 0, global_rate: 10, global_burst: 10),
    limit.Config(sender_rate: 5, sender_burst: 5, global_rate: limit.MAX_RATE + 1, global_burst: 10),
    limit.Config(sender_rate: 5, sender_burst: 5, global_rate: 10, global_burst: limit.MAX_BURST + 1),
    limit.Config(sender_rate: -1, sender_burst: 5, global_rate: 10, global_burst: 10),
  ] {
    assert limit.new_limiter(config) is Err(_)
  }
  assert limit.new_limiter(limit.DEFAULT_CONFIG) is Ok(_)
  assert limit.new_limiter(limit.Config(sender_rate: limit.MAX_RATE, sender_burst: limit.MAX_BURST, global_rate: 1, global_burst: 1)) is Ok(_)
}

test test_a_sender_may_burst_then_is_held_to_its_sustained_rate {
  var l = limit.new_limiter(TIGHT)?
  var accepted = 0
  for step in range(5) {
    let decision = limit.admit(l, plain(1, 7), 0)
    l = decision.limiter
    if decision.accepted { accepted += 1 }
  }
  assert accepted == 5
  let refused = limit.admit(l, plain(1, 7), 0)
  assert ! refused.accepted and refused.reason == .Rate
  l = refused.limiter
  let early = limit.admit(l, plain(1, 7), 99999)
  assert ! early.accepted
  let on_time = limit.admit(early.limiter, plain(1, 7), 100000)
  assert on_time.accepted
  assert ! limit.admit(on_time.limiter, plain(1, 7), 100001).accepted
}

test test_accepted_count_over_time_matches_burst_plus_rate {
  var l = limit.new_limiter(TIGHT)?
  var accepted = 0
  for step in range(1000) {
    let decision = limit.admit(l, plain(1, 7), step * 10000)
    l = decision.limiter
    if decision.accepted { accepted += 1 }
  }
  assert accepted >= 5 + 10 * 10 - 1 and accepted <= 5 + 10 * 10 + 1, f"{accepted}"
  assert l.counters.offered == 1000 and l.counters.rate == 1000 - accepted
}

test test_a_flooding_sender_does_not_spend_another_senders_budget {
  var l = limit.new_limiter(TIGHT)?
  for step in range(500) { l = limit.admit(l, plain(1, 7), 0).limiter }
  let other = limit.admit(l, plain(2, 8), 0)
  assert other.accepted
}

test test_the_global_ceiling_limits_all_senders_together_and_refusals_cost_nothing {
  let config = limit.Config(sender_rate: 1000, sender_burst: 1000, global_rate: 10, global_burst: 3)
  var l = limit.new_limiter(config)?
  var accepted = 0
  for sender in range(20) {
    let decision = limit.admit(l, plain(sender, 1), 0)
    l = decision.limiter
    if decision.accepted { accepted += 1 }
  }
  assert accepted == 3
  assert l.senders.len() == 3
  assert limit.admit(l, plain(0, 1), 100000).accepted
}

test test_oversize_truncated_control_and_descriptor_datagrams_drop_without_using_budget {
  var l = limit.new_limiter(TIGHT)?
  let cases = [
    limit.Datagram(sender: limit.Sender(uid: 1, pid: 7), oversize: true, control_truncated: false, rights: 0),
    limit.Datagram(sender: limit.Sender(uid: 1, pid: 7), oversize: false, control_truncated: true, rights: 0),
    limit.Datagram(sender: limit.Sender(uid: 1, pid: 7), oversize: false, control_truncated: false, rights: 3),
  ]
  for round in range(100) {
    for datagram in cases { l = limit.admit(l, datagram, 0).limiter }
  }
  assert l.counters.oversize == 100 and l.counters.control_truncated == 100 and l.counters.rights == 100
  assert l.counters.accepted == 0 and l.counters.offered == 300
  assert l.senders.len() == 0
  assert limit.admit(l, plain(1, 7), 0).accepted
}

test test_each_datagram_with_several_problems_is_counted_under_exactly_one_reason {
  let all = limit.Datagram(sender: null, oversize: true, control_truncated: true, rights: 2)
  let l = limit.admit(limit.new_limiter(TIGHT)?, all, 0).limiter
  assert l.counters.offered == 1 and dropped_total(l) == 1 and l.counters.rights == 1
}

test test_senders_without_credentials_share_one_bucket {
  var l = limit.new_limiter(TIGHT)?
  let anonymous = limit.Datagram(sender: null, oversize: false, control_truncated: false, rights: 0)
  var accepted = 0
  for step in range(50) {
    let decision = limit.admit(l, anonymous, 0)
    l = decision.limiter
    if decision.accepted { accepted += 1 }
  }
  assert accepted == 5 and l.senders.len() == 1
}

test test_sender_table_stays_bounded_under_churn_and_evicts_the_least_recently_used {
  var l = limit.new_limiter(limit.Config(sender_rate: 1000, sender_burst: 1000, global_rate: limit.MAX_RATE, global_burst: limit.MAX_BURST))?
  for pid in range(limit.MAX_SENDERS) { l = limit.admit(l, plain(1, pid), pid).limiter }
  assert l.senders.len() == limit.MAX_SENDERS
  for pid in range(limit.MAX_SENDERS) {
    if pid != 17 { l = limit.admit(l, plain(1, pid), 1000 + pid).limiter }
  }
  l = limit.admit(l, plain(2, 0), 5000).limiter
  assert l.senders.len() == limit.MAX_SENDERS and l.counters.evicted_senders == 1
  assert "1:17" not in l.senders and "1:16" in l.senders and "2:0" in l.senders
  for pid in range(3000) { l = limit.admit(l, plain(3, pid), 10000 + pid).limiter }
  assert l.senders.len() == limit.MAX_SENDERS and l.order.len() == limit.MAX_SENDERS
  assert l.counters.evicted_senders == 1 + 3000
}

test test_a_busy_sender_survives_churn_by_others {
  var l = limit.new_limiter(limit.Config(sender_rate: 1000, sender_burst: 1000, global_rate: limit.MAX_RATE, global_burst: limit.MAX_BURST))?
  for pid in range(1500) {
    l = limit.admit(l, plain(9, 1), pid).limiter
    l = limit.admit(l, plain(1, pid), pid).limiter
  }
  assert "9:1" in l.senders
}

proc generated_flood(seed: Int, count: Int) -> Result[limit.Limiter, Error] {
  var l = limit.new_limiter(limit.Config(sender_rate: 20, sender_burst: 10, global_rate: 200, global_burst: 50))?
  var state = seed
  var now = 0
  for _ in range(count) {
    state = (state * 1103515245 + 12345) % 2147483648
    now += state / 4096 % 5000
    let kind = state / 65536 % 20
    let datagram = limit.Datagram(
      sender: if kind == 19 { null } else { limit.Sender(uid: state / 256 % 7, pid: state / 16 % 400) },
      oversize: kind == 17,
      control_truncated: kind == 18,
      rights: if kind == 16 { 2 } else { 0 },
    )
    l = limit.admit(l, datagram, now).limiter
  }
  Ok(l)
}

test test_counters_sum_to_the_offered_load_in_a_fixed_seed_flood {
  let l = generated_flood(20261028, 2000)?
  assert l.counters.offered == 2000
  assert l.counters.accepted + dropped_total(l) == 2000
  assert l.senders.len() <= limit.MAX_SENDERS and l.order.len() <= limit.MAX_SENDERS
  assert l.counters.accepted > 0 and l.counters.rate > 0 and l.counters.oversize > 0
  assert l.counters.control_truncated > 0 and l.counters.rights > 0
}

test test_sink_drops_move_admitted_datagrams_into_their_own_reason {
  var l = limit.new_limiter(TIGHT)?
  for pid in range(4) { l = limit.admit(l, plain(1, pid), 0).limiter }
  l = limit.sink_dropped(l, .SinkBlocked, 3)
  l = limit.sink_dropped(l, .SinkError, 1)
  assert l.counters.accepted == 0 and l.counters.sink_blocked == 3 and l.counters.sink_error == 1
  assert l.counters.offered == l.counters.accepted + dropped_total(l)
}

test test_time_extremes_never_fail_or_overflow {
  var l = limit.new_limiter(TIGHT)?
  for now in [-1, 0, 9223372036854775807, -9223372036854775807, 4611686018427387903, 4611686018427387904] {
    for step in range(20) { l = limit.admit(l, plain(1, 7), now).limiter }
  }
  assert l.counters.offered == 120
  assert l.counters.accepted + dropped_total(l) == 120
  assert l.counters.accepted >= 5
}

test test_the_summary_reports_kernel_loss_as_unobservable {
  let summary = limit.summary(limit.new_limiter(TIGHT)?)
  assert "kernel_loss=unobservable" in summary and "offered=0" in summary and "dropped_sink_blocked=0" in summary
}
