##! Measures elapsed time and deadlines on the process monotonic clock.
##!
##! A Reading is whole milliseconds from the process-local origin of
##! `time.monotonic()`; it is never wall time and never leaves the process. A
##! Deadline is a Reading plus a budget. The two are nominal types, so a wall
##! timestamp from `time.now()` cannot be compared with either.
const MAX_MS = 9223372036854775807

## Holds milliseconds on the process monotonic clock.
export nominal type Reading = {ms: Int}
## Holds the monotonic millisecond at which a budget runs out.
export nominal type Deadline = {at_ms: Int}

## Reads the monotonic clock in milliseconds.
export proc now() [time, error] -> Result[Reading, Error] {
  Ok(Reading(ms: time.monotonic()? / 1000000))
}

## Converts whole seconds to milliseconds, saturating past the integer range.
export pure seconds_ms(seconds: Int) -> Int {
  if seconds > MAX_MS / 1000 { MAX_MS } else { seconds * 1000 }
}

## Ends a budget of milliseconds after a reading; a negative budget is zero and a sum past the integer range saturates.
export pure deadline_after(start: Reading, budget_ms: Int) -> Deadline {
  let budget = if budget_ms < 0 { 0 } else { budget_ms }
  Deadline(at_ms: if budget > MAX_MS - start.ms { MAX_MS } else { start.ms + budget })
}

## Reports the milliseconds left before a deadline, never below zero.
export pure remaining_ms(deadline: Deadline, now: Reading) -> Int {
  if now.ms >= deadline.at_ms { 0 } else { deadline.at_ms - now.ms }
}

## Reports whether the budget is spent; a zero budget is spent at once.
export pure expired(deadline: Deadline, now: Reading) -> Bool {
  now.ms >= deadline.at_ms
}

## Reports the milliseconds since a reading, never below zero.
export pure since_ms(start: Reading, now: Reading) -> Int {
  if now.ms <= start.ms { 0 } else { now.ms - start.ms }
}
