##! Tracks sample deadlines independently from collection completion times.
##!
##! Deadlines advance from the original cadence; missed intervals are reported
##! on the next due sample instead of triggering a catch-up burst.

const MAX_TIME_NS = 9223372036854775807

## Holds one absolute deadline cadence.
export type State = {interval_ns: Int, next_due_ns: Int}

## Identifies when one due sample was scheduled and actually started.
export type Ticket = {deadline_ns: Int, started_ns: Int, coalesced: Int}

## Records completion without changing the next sampling deadline.
export type Completion = {ticket: Ticket, completed_ns: Int, duration_ns: Int}

## Returns the immediate first sample and its following schedule.
export type Start = {state: State, ticket: Ticket}

## Returns a claimed sample only after its absolute deadline.
export type Due = {state: State, ticket: Ticket?}

## Starts the first sample now and establishes the next absolute deadline.
export pure first(started_ns: Int, interval_ns: Int) -> Result[Start, Error] {
  if started_ns < 0 or interval_ns <= 0 or interval_ns > MAX_TIME_NS - started_ns {
    return Err(error.failure("sample schedule requires a valid monotonic time and interval"))
  }
  let ticket = {deadline_ns: started_ns, started_ns: started_ns, coalesced: 0}
  let state = {interval_ns: interval_ns, next_due_ns: started_ns + interval_ns}
  Ok({state: state, ticket: ticket})
}

## Claims the newest elapsed deadline and reports every older one as coalesced.
export pure take_due(state: State, now_ns: Int) -> Result[Due, Error] {
  if state.interval_ns <= 0 or state.next_due_ns < 0 or now_ns < 0 {
    return Err(error.failure("sample schedule contains an invalid monotonic time"))
  }
  if now_ns < state.next_due_ns {
    return Ok({state: state, ticket: null})
  }
  if now_ns > MAX_TIME_NS - state.interval_ns {
    return Err(error.failure("sample schedule reached the monotonic time limit"))
  }
  let coalesced = (now_ns - state.next_due_ns) / state.interval_ns
  let deadline_ns = state.next_due_ns + coalesced * state.interval_ns
  let ticket = {deadline_ns: deadline_ns, started_ns: now_ns, coalesced: coalesced}
  Ok({
    state: {interval_ns: state.interval_ns, next_due_ns: deadline_ns + state.interval_ns},
    ticket: ticket,
  })
}

## Records completed collection work separately from its scheduled deadline.
export pure complete(ticket: Ticket, completed_ns: Int) -> Result[Completion, Error] {
  if ticket.deadline_ns < 0 or ticket.started_ns < ticket.deadline_ns or
      ticket.coalesced < 0 or completed_ns < ticket.started_ns {
    return Err(error.failure("sample completion precedes its scheduled work"))
  }
  Ok({ticket: ticket, completed_ns: completed_ns, duration_ns: completed_ns - ticket.started_ns})
}
