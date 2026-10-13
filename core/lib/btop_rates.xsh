##! Pure interval math for CPU, process, network, and memory panel values.
##!
##! Counter changes and elapsed time are validated before division so a first
##! sample, reset, or invalid clock interval cannot masquerade as inactivity.
use sys_cpu as syscpu

## Names the temporal validity of a derived interval value.
export enum RateState: Str {
  Warming = "warming",
  Observed = "observed",
  Reset = "reset",
  Invalid = "invalid",
  Gap = "gap",
  Unavailable = "unavailable",
}

## Holds a floating-point rate or percentage with its temporal state.
export type Rate = {state: RateState, value: Float?}
## Holds an exact byte value with its availability state.
export type ByteValue = {state: RateState, value: Int?}

const max_counter = 9223372036854775807

pure add_checked(left: Int, right: Int) -> Int? {
  return null when left < 0 or right < 0 or left > max_counter - right
  left + right
}

pure sum_checked(values: List[Int]) -> Int? {
  var total = 0
  for value in values {
    let next = add_checked(total, value)
    return null when next == null
    total = next
  }
  total
}

pure counter_at(fields: List[Int], index: Int) -> Int {
  fields.get(index) ?? 0
}

type CounterDelta = {state: RateState, delta: Int}

pure counter_delta(current: List[Int], previous: List[Int], index: Int, correct_iowait: Bool) -> CounterDelta {
  let now = counter_at(current, index)
  let before = counter_at(previous, index)
  if now < before {
    return {state: .Observed, delta: 0} when index == 4 and correct_iowait
    return {state: .Reset, delta: 0}
  }
  {state: .Observed, delta: now - before}
}

type AggregateDelta = {state: RateState, total_ticks: Int?, busy_ticks: Int?, idle_ticks: Int?}

pure aggregate_delta(current: syscpu.CpuCounters, previous: syscpu.CpuCounters, correct_iowait: Bool) -> AggregateDelta {
  if current.fields.len() < 4 or previous.fields.len() < 4 {
    return {state: .Invalid, total_ticks: null, busy_ticks: null, idle_ticks: null}
  }
  var deltas: List[Int] = []
  for index in range(8) {
    let item = counter_delta(current.fields, previous.fields, index, correct_iowait)
    if item.state == .Reset {
      return {state: .Reset, total_ticks: null, busy_ticks: null, idle_ticks: null}
    }
    deltas += [item.delta]
  }
  let guest = counter_delta(current.fields, previous.fields, 8, false)
  let guest_nice = counter_delta(current.fields, previous.fields, 9, false)
  if guest.state == .Reset or guest_nice.state == .Reset {
    return {state: .Reset, total_ticks: null, busy_ticks: null, idle_ticks: null}
  }
  if guest.delta > deltas[0] or guest_nice.delta > deltas[1] {
    return {state: .Invalid, total_ticks: null, busy_ticks: null, idle_ticks: null}
  }
  deltas[0] -= guest.delta
  deltas[1] -= guest_nice.delta
  let busy = sum_checked([deltas[0], deltas[1], deltas[2], deltas[5], deltas[6], deltas[7]])
  let idle = sum_checked([deltas[3], deltas[4]])
  if busy == null or idle == null {
    return {state: .Invalid, total_ticks: null, busy_ticks: null, idle_ticks: null}
  }
  let total = add_checked(busy, idle)
  if total == null or (total) == 0 {
    return {state: .Invalid, total_ticks: null, busy_ticks: null, idle_ticks: null}
  }
  {state: .Observed, total_ticks: total, busy_ticks: busy, idle_ticks: idle}
}

## Computes aggregate busy time; the first observation is deliberately warming.
export pure cpu_percent(current: syscpu.CpuCounters, previous: syscpu.CpuCounters?) -> Rate {
  return {state: .Warming, value: null} when previous == null
  let delta = aggregate_delta(current, previous ?? current, true)
  return {state: delta.state, value: null} when delta.state != .Observed
  let busy = delta.busy_ticks ?? 0
  let total = delta.total_ticks ?? 0
  {state: .Observed, value: busy.float() * 100.0 / total.float()}
}

## Computes process CPU using the all-CPU first-eight-field denominator.
export pure process_percent(
  current_ticks: Int,
  previous_ticks: Int?,
  current_cpu: syscpu.CpuCounters,
  previous_cpu: syscpu.CpuCounters?,
  core_count: Int,
  per_core: Bool = false,
) -> Rate {
  if previous_ticks == null or previous_cpu == null {
    return {state: .Warming, value: null}
  }
  if current_ticks < 0 or (previous_ticks ?? -1) < 0 or current_ticks < (previous_ticks) {
    return {state: .Reset, value: null}
  }
  return {state: .Invalid, value: null} when core_count <= 0
  let delta = aggregate_delta(current_cpu, previous_cpu ?? current_cpu, false)
  return {state: delta.state, value: null} when delta.state != .Observed
  let total = delta.total_ticks ?? 0
  return {state: .Invalid, value: null} when total <= 0
  let ticks = current_ticks - (previous_ticks)
  let scale = if per_core { core_count.float() } else { 1.0 }
  let raw = ticks.float() * scale * 100.0 / total.float()
  let ceiling = 100.0 * core_count.float()
  let bounded = if raw < 0.0 { 0.0 } else if raw > ceiling { ceiling } else { raw }
  let rounded = (bounded * 10.0).round()
  if let Ok(tenth) = rounded {
    {state: .Observed, value: tenth.float() / 10.0}
  } else {
    {state: .Invalid, value: null}
  }
}

## Computes a byte-per-second rate from a positive monotonic interval.
export pure counter_rate(current: Int, previous: Int, elapsed_ns: Int) -> Rate {
  if current < 0 or previous < 0 or elapsed_ns <= 0 {
    return {state: .Invalid, value: null}
  }
  return {state: .Reset, value: null} when current < previous
  let rate = (current - previous).float() * 1000000000.0 / elapsed_ns.float()
  {state: .Observed, value: rate}
}

## Computes used memory only when both independent counters are observed.
export pure used_bytes(total: Int?, available: Int?) -> ByteValue {
  return {state: .Unavailable, value: null} when total == null or available == null
  let total_value = total ?? -1
  let available_value = available ?? -1
  if total_value < 0 or available_value < 0 or available_value > total_value {
    return {state: .Invalid, value: null}
  }
  {state: .Observed, value: total_value - available_value}
}

## Converts a resident-page count without overflowing the signed counter type.
export pure pages_to_bytes(pages: Int, page_size_bytes: Int) -> Int? {
  return null when pages < 0 or page_size_bytes <= 0
  return null when pages > max_counter / page_size_bytes
  pages * page_size_bytes
}
