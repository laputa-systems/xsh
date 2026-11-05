use core.lib.monotonic

pure at(ms: Int) -> monotonic.Reading {
  monotonic.Reading(ms: ms)
}

test test_a_budget_expires_exactly_when_it_is_spent {
  let start = at(1000)
  let deadline = monotonic.deadline_after(start, 250)
  assert deadline.at_ms == 1250
  assert ! monotonic.expired(deadline, at(1249))
  assert monotonic.remaining_ms(deadline, at(1249)) == 1
  assert monotonic.expired(deadline, at(1250))
  assert monotonic.remaining_ms(deadline, at(1250)) == 0
  assert monotonic.remaining_ms(deadline, at(9000)) == 0
}

test test_a_zero_or_negative_budget_is_expired_at_once {
  let start = at(1000)
  assert monotonic.expired(monotonic.deadline_after(start, 0), start)
  assert monotonic.expired(monotonic.deadline_after(start, -5), start)
  assert monotonic.remaining_ms(monotonic.deadline_after(start, -5), start) == 0
}

test test_an_enormous_budget_saturates_instead_of_overflowing {
  let start = at(1000)
  let deadline = monotonic.deadline_after(start, 9223372036854775807)
  assert deadline.at_ms == 9223372036854775807
  assert ! monotonic.expired(deadline, at(9000000000000))
  assert monotonic.remaining_ms(deadline, start) == 9223372036854774807
}

test test_seconds_convert_to_milliseconds_and_saturate {
  assert monotonic.seconds_ms(0) == 0
  assert monotonic.seconds_ms(120) == 120000
  assert monotonic.seconds_ms(9223372036854775) == 9223372036854775000
  assert monotonic.seconds_ms(9223372036854776) == 9223372036854775807
}

test test_elapsed_time_never_goes_negative {
  assert monotonic.since_ms(at(1000), at(1750)) == 750
  assert monotonic.since_ms(at(1750), at(1000)) == 0
  assert monotonic.since_ms(at(1000), at(1000)) == 0
}

test test_the_live_reading_is_monotonic_milliseconds {
  let first = monotonic.now()?
  let second = monotonic.now()?
  assert monotonic.since_ms(first, second) < 60000
  assert second.ms >= first.ms
}

test test_a_wall_clock_value_is_not_a_reading { |ctx|
  test.expect(ctx, r"""use core.lib.monotonic
let deadline = monotonic.deadline_after(monotonic.Reading(ms: 0), 10)
let wall: Int = time.now()
let spent = monotonic.expired(deadline, wall)
""", status: 2, stderr: ["[check.type-mismatch]"])?
}
