use core.lib.btop_schedule as schedule

test test_btop_schedule_starts_all_classes_and_uses_fixed_periods {
  let first = schedule.initial(1000)?
  let due = schedule.take_due(first, 1000)?
  assert (due.due |> map .class) == [schedule.Inventory, schedule.Counters, schedule.Gauges, schedule.Blocking]
  assert (due.state.entries |> map .next_due_ns) == [30000001000, 1000001000, 5000001000, 30000001000]
}

test test_btop_schedule_coalesces_each_class_without_catch_up_work {
  let first = schedule.initial(0)?
  let started = schedule.take_due(first, 0)?
  let counters = schedule.take_due(started.state, 8500000000)?
  assert counters.due.len() == 2
  assert counters.due[0].class == schedule.Counters
  assert counters.due[1].class == schedule.Gauges
  assert counters.due[0].deadline_ns == 8000000000
  assert counters.due[0].coalesced == 7
  assert counters.state.entries[1].next_due_ns == 9000000000
  assert counters.state.entries[2].next_due_ns == 10000000000
}

test test_btop_schedule_invalidates_inventory_before_its_ttl {
  let started = schedule.take_due(schedule.initial(0)?, 0)?
  let invalidated = schedule.invalidate_inventory(started.state, 1000000000)?
  let plan = schedule.take_due(invalidated, 1000000000)?
  assert (plan.due |> map .class) == [schedule.Inventory, schedule.Counters]
  assert plan.due[0] == {class: schedule.Inventory, deadline_ns: 1000000000, coalesced: 0}
  assert plan.state.entries[0].next_due_ns == 31000000000
}

test test_btop_schedule_rejects_invalid_clock_and_deadline_overflow {
  assert schedule.initial(-1) is Err(_)
  assert schedule.take_due({entries: [{class: schedule.Counters, interval_ns: 0, next_due_ns: 1, invalidated: false}]}, 1) is Err(_)
  assert schedule.take_due({entries: [{class: schedule.Counters, interval_ns: 1000000000, next_due_ns: 9223372036854775807, invalidated: false}]}, 9223372036854775807) is Err(_)
}
