##! Assigns fixed collection cadences to independently scheduled Linux inputs.
##!
##! Counters follow the visible refresh cadence; gauges and blocking capacity
##! calls run less often so slow sources cannot delay every terminal redraw.
const MAX_TIME_NS = 9223372036854775807

## Names one independently scheduled source class.
export enum Class: Str {
  Inventory = "inventory",
  Counters = "counters",
  Gauges = "gauges",
  Blocking = "blocking",
}

## Holds one absolute deadline for a source class.
export type Entry = {class: Class, interval_ns: Int, next_due_ns: Int, invalidated: Bool}
## Holds the next deadline for each bounded source class.
export type State = {entries: List[Entry]}
## Reports the due source class and the coalesced deadlines before it.
export type Due = {class: Class, deadline_ns: Int, coalesced: Int}
## Carries due work and the advanced schedule together.
export type Plan = {state: State, due: List[Due]}

pure interval(class: Class) -> Int {
  match class {
    Inventory => 30000000000,
    Counters => 1000000000,
    Gauges => 5000000000,
    Blocking => 30000000000,
  }
}

## Starts all source classes due at the first monotonic observation.
export pure initial(now_ns: Int) -> Result[State, Error] {
  if now_ns < 0 or now_ns > MAX_TIME_NS - interval(Blocking) {
    return Err(error.failure("collection schedule requires a nonnegative monotonic time"))
  }
  let entries: List[Entry] = collect {
    for class in [Inventory, Counters, Gauges, Blocking] {
      yield {class: class, interval_ns: interval(class), next_due_ns: now_ns, invalidated: false}
    }
  }
  Ok({entries: entries})
}

## Forces an inventory refresh after topology or identity changes.
export pure invalidate_inventory(state: State, now_ns: Int) -> Result[State, Error] {
  if now_ns < 0 or now_ns > MAX_TIME_NS - interval(Blocking) {
    return Err(error.failure("inventory invalidation requires a nonnegative monotonic time"))
  }
  var entries: List[Entry] = []
  var found = false
  for entry in state.entries {
    if entry.class == Inventory {
      entries += [{class: entry.class, interval_ns: entry.interval_ns, next_due_ns: now_ns, invalidated: true}]
      found = true
    } else {
      entries += [entry]
    }
  }
  if ! found {
    return Err(error.failure("collection schedule is missing its inventory class"))
  }
  Ok({entries: entries})
}

## Claims each due class once and coalesces missed periods without catch-up work.
export pure take_due(state: State, now_ns: Int) -> Result[Plan, Error] {
  if now_ns < 0 or state.entries.len() != 4 {
    return Err(error.failure("collection schedule requires a nonnegative monotonic time"))
  }
  for index in range(state.entries.len()) {
    guard state.entries[index].interval_ns == interval(state.entries[index].class) else {
      return Err(error.failure("collection schedule has a mismatched class interval"))
    }
    for prior in range(index) {
      if state.entries[prior].class == state.entries[index].class {
        return Err(error.failure("collection schedule repeats a source class"))
      }
    }
  }
  var entries: List[Entry] = []
  var due: List[Due] = []
  for entry in state.entries {
    if entry.next_due_ns < 0 or entry.next_due_ns > MAX_TIME_NS - entry.interval_ns {
      return Err(error.failure("collection schedule contains an invalid deadline"))
    }
    if entry.invalidated or now_ns >= entry.next_due_ns {
      let deadline_ns = if entry.invalidated { now_ns } else { entry.next_due_ns }
      let coalesced = if entry.invalidated or now_ns < entry.next_due_ns { 0 } else { (now_ns - entry.next_due_ns) / entry.interval_ns }
      if coalesced + 1 > (MAX_TIME_NS - deadline_ns) / entry.interval_ns {
        return Err(error.failure("collection schedule reached the monotonic time limit"))
      }
      due += [{class: entry.class, deadline_ns: deadline_ns + coalesced * entry.interval_ns, coalesced: coalesced}]
      entries += [{class: entry.class, interval_ns: entry.interval_ns, next_due_ns: deadline_ns + (coalesced + 1) * entry.interval_ns, invalidated: false}]
    } else {
      entries += [entry]
    }
  }
  Ok({state: {entries: entries}, due: due})
}
