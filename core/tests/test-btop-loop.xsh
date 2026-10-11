use core.lib.btop_loop as schedule

test test_btop_loop_separates_due_and_completed_sample_times {
  let started = schedule.first(10000, 1000)?
  assert started.ticket.deadline_ns == 10000
  assert started.ticket.started_ns == 10000
  assert started.state.next_due_ns == 11000

  let before_deadline = schedule.take_due(started.state, 10999)?
  assert before_deadline.ticket == null
  assert before_deadline.state == started.state

  let due = schedule.take_due(started.state, 11250)?
  let ticket = due.ticket ?? {deadline_ns: 0, started_ns: 0, coalesced: 0}
  assert ticket.deadline_ns == 11000
  assert ticket.started_ns == 11250
  assert ticket.coalesced == 0
  assert due.state.next_due_ns == 12000

  let completed = schedule.complete(ticket, 11700)?
  assert completed.completed_ns == 11700
  assert completed.duration_ns == 450
}

test test_btop_loop_coalesces_overdue_deadlines_without_catch_up_bursts {
  let started = schedule.first(0, 1000000000)?
  let first = schedule.take_due(started.state, 1000000000)?
  let first_ticket = first.ticket ?? {deadline_ns: -1, started_ns: -1, coalesced: -1}
  let slow_completion = schedule.complete(first_ticket, 4500000000)?
  assert slow_completion.duration_ns == 3500000000

  let overdue = schedule.take_due(first.state, slow_completion.completed_ns)?
  let overdue_ticket = overdue.ticket ?? {deadline_ns: 0, started_ns: 0, coalesced: 0}
  assert overdue_ticket.deadline_ns == 4000000000
  assert overdue_ticket.started_ns == 4500000000
  assert overdue_ticket.coalesced == 2
  assert overdue.state.next_due_ns == 5000000000

  assert schedule.take_due(overdue.state, 4999999999)?.ticket == null
  let next = schedule.take_due(overdue.state, 5000000000)?
  let next_ticket = next.ticket ?? {deadline_ns: 0, started_ns: 0, coalesced: 0}
  assert next_ticket.deadline_ns == 5000000000
  assert next_ticket.coalesced == 0
}

test test_btop_loop_rejects_invalid_times_intervals_and_completion_order {
  assert schedule.first(-1, 1) is Err(_)
  assert schedule.first(0, 0) is Err(_)
  assert schedule.first(9223372036854775807, 1) is Err(_)
  assert schedule.take_due({interval_ns: 0, next_due_ns: 10}, 10) is Err(_)
  assert schedule.take_due({interval_ns: 10, next_due_ns: 10}, -1) is Err(_)
  assert schedule.complete({deadline_ns: 10, started_ns: 10, coalesced: 0}, 9) is Err(_)
}
