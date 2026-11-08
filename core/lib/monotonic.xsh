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

## Holds nanoseconds on the process monotonic clock, for round trips too short for a Reading.
export nominal type Stamp = {ns: Int}

## Reads the monotonic clock in nanoseconds.
export proc stamp() [time, error] -> Result[Stamp, Error] {
  Ok(Stamp(ns: time.monotonic()?))
}

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

## Reports the nanoseconds between two stamps, never below zero.
export pure elapsed_ns(start: Stamp, end: Stamp) -> Int {
  if end.ns <= start.ns { 0 } else { end.ns - start.ns }
}

## Reports a round trip in nanoseconds from two precise wall readings, kept inside what the monotonic stamps allow.
## The wall readings resolve microseconds but a clock step moves them; the stamps taken around the same events cannot
## step and bound the true round trip from above. A wall difference that is negative or longer than that bound proves a
## step, and the bound answers instead.
export pure round_trip_ns(wall_sent_ns: Int, wall_received_ns: Int, sent: Stamp, received: Stamp) -> Int {
  let bound = elapsed_ns(sent, received)
  let wall = wall_received_ns - wall_sent_ns
  if wall < 0 or wall > bound { bound } else { wall }
}
