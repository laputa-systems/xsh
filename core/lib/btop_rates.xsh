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
  if total == null or total == 0 {
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
  per_core = false,
) -> Rate {
  if previous_ticks == null or previous_cpu == null {
    return {state: .Warming, value: null}
  }
  let percents = process_percents(
    current_ticks,
    previous_ticks,
    process_total(current_cpu, previous_cpu ?? current_cpu),
    core_count,
  )
  if per_core { percents.per_core } else { percents.share }
}

## Holds the aggregate CPU ticks one interval offers to every process in it.
export type ProcessTotal = {state: RateState, total_ticks: Int}

## Holds one process CPU rate as a share of all CPUs and as a share of one core.
export type ProcessPercents = {share: Rate, per_core: Rate}

## Derives the interval's aggregate ticks once so each process need not repeat it.
export pure process_total(current_cpu: syscpu.CpuCounters, previous_cpu: syscpu.CpuCounters) -> ProcessTotal {
  let delta = aggregate_delta(current_cpu, previous_cpu, false)
  return {state: delta.state, total_ticks: 0} when delta.state != .Observed
  let total = delta.total_ticks ?? 0
  return {state: .Invalid, total_ticks: 0} when total <= 0
  {state: .Observed, total_ticks: total}
}

## Computes both process CPU rates from a precomputed interval total.
export pure process_percents(
  current_ticks: Int,
  previous_ticks: Int?,
  total: ProcessTotal,
  core_count: Int,
) -> ProcessPercents {
  if previous_ticks == null {
    return {share: {state: .Warming, value: null}, per_core: {state: .Warming, value: null}}
  }
  if current_ticks < 0 or (previous_ticks ?? -1) < 0 or current_ticks < previous_ticks {
    return {share: {state: .Reset, value: null}, per_core: {state: .Reset, value: null}}
  }
  if core_count <= 0 {
    return {share: {state: .Invalid, value: null}, per_core: {state: .Invalid, value: null}}
  }
  if total.state != .Observed {
    return {share: {state: total.state, value: null}, per_core: {state: total.state, value: null}}
  }
  let ticks = (current_ticks - previous_ticks).float()
  let cores = core_count.float()
  let divisor = total.total_ticks.float()
  let ceiling = 100.0 * cores
  let raw_share = ticks * 1.0 * 100.0 / divisor
  let raw_per_core = ticks * cores * 100.0 / divisor
  let share_tenths = ((if raw_share < 0.0 { 0.0 } else if raw_share > ceiling { ceiling } else { raw_share }) * 10.0).round()
  let per_core_tenths = ((if raw_per_core < 0.0 { 0.0 } else if raw_per_core > ceiling { ceiling } else { raw_per_core }) * 10.0).round()
  {
    share: if let Ok(tenth) = share_tenths {
      {state: .Observed, value: tenth.float() / 10.0}
    } else {
      {state: .Invalid, value: null}
    },
    per_core: if let Ok(tenth) = per_core_tenths {
      {state: .Observed, value: tenth.float() / 10.0}
    } else {
      {state: .Invalid, value: null}
    },
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
