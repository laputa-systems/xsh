##! Derives one monitor frame from raw rooted observations and a prior frame.
##!
##! The model owns interval semantics and stable process ranking; neither the
##! renderer nor replay decides whether a counter represents a reset or rate.
use btop_rates as rates
use sys_cpu as syscpu
use sys_mem as memory
use sys_net as network
use sys_proc as processes
use system_report as report

## Groups one timestamp with complete source results from a single rooted tree.
export type Observation = {
  source_id: Str,
  at_ns: Int,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
  cpu: syscpu.CpuSource,
  memory: memory.MemorySource,
  network: network.NetworkSource,
  processes: processes.ProcessSource,
}

## Retains one process row and its interval percentage before terminal formatting.
export type ProcessRow = {
  pid: Int,
  parent_pid: Int,
  start_ticks: Int,
  thread_count: Int?,
  uid: Int?,
  uid_state: MetadataState,
  command: Str,
  cpu_percent: rates.Rate,
  cpu_average_percent: rates.Rate,
  rss_bytes: Int?,
}

## Retains one sparse Linux logical CPU identifier and interval utilization.
export type CoreUsage = {id: Int, utilization: rates.Rate}

## Distinguishes current UID observations from a bounded stale reuse.
export enum MetadataState: Str {
  Fresh = "fresh",
  Stale = "stale",
  Unavailable = "unavailable",
}

## Retains one process UID only while its identity and command remain stable.
export type ProcessMetadata = {start_ticks: Int, command: Str, uid: Int, observed_at_ns: Int}
## Bounds metadata reuse to one source session and the current process inventory.
export type MetadataCache = {source_id: Str, by_pid: Map[Int, ProcessMetadata]}

## Keeps values and source states distinct for the memory panel.
export type MemoryPanel = {
  total_bytes: Int?,
  available_bytes: Int?,
  used_bytes: rates.ByteValue,
  swap_total_bytes: Int?,
  swap_free_bytes: Int?,
  swap_used_bytes: rates.ByteValue,
  total_state: report.ObservationState,
  available_state: report.ObservationState,
  swap_total_state: report.ObservationState,
  swap_free_state: report.ObservationState,
}

## Keeps one deterministically selected interface and its interval byte rates.
export type NetworkPanel = {
  source_state: report.ObservationState,
  interface_name: Str?,
  rx_bytes: Int?,
  tx_bytes: Int?,
  rx_bytes_per_second: rates.Rate,
  tx_bytes_per_second: rates.Rate,
}

## Holds the display-ready values for one sampling instant.
export type Model = {
  source_id: Str,
  at_ns: Int,
  cpu_percent: rates.Rate,
  cpu_cores: List[CoreUsage],
  uptime: syscpu.Uptime,
  load: syscpu.LoadAverage,
  cpu_state: report.ObservationState,
  memory: MemoryPanel,
  network: NetworkPanel,
  process_state: report.ObservationState,
  processes: List[ProcessRow],
}

## Returns the display model and the next bounded metadata cache together.
export type CachedStep = {value: Model, metadata: MetadataCache}

pure rate(state: rates.RateState) -> rates.Rate {
  {state: state, value: null}
}

pure same_source_baseline(current: Observation, previous: Observation?) -> Observation? {
  if previous == null {
    return null
  }

  let before = previous ?? current
  if before.source_id != current.source_id {
    return null
  }

  before
}

pure observed_interface(source: network.NetworkSource) -> network.NetworkInterface? {
  if source.state == report.Observed { network.selected_interface(source.interfaces) } else { null }
}

pure memory_panel(source: memory.MemorySource) -> MemoryPanel {
  {
    total_bytes: source.total.bytes,
    available_bytes: source.available.bytes,
    used_bytes: rates.used_bytes(source.total.bytes, source.available.bytes),
    swap_total_bytes: source.swap_total.bytes,
    swap_free_bytes: source.swap_free.bytes,
    swap_used_bytes: rates.used_bytes(source.swap_total.bytes, source.swap_free.bytes),
    total_state: source.total.state,
    available_state: source.available.state,
    swap_total_state: source.swap_total.state,
    swap_free_state: source.swap_free.state,
  }
}

pure cpu_interval(current: Observation, previous: Observation?) -> rates.Rate {
  if current.cpu.stat.aggregate == null {
    return rate(if current.cpu.stat.state == report.Observed { rates.Invalid } else { rates.Unavailable })
  }

  if previous == null {
    return rate(rates.Warming)
  }

  let before = previous ?? current
  if current.at_ns <= before.at_ns {
    return rate(rates.Invalid)
  }

  if before.cpu.stat.aggregate == null {
    return rate(rates.Warming)
  }

  rates.cpu_percent(current.cpu.stat.aggregate, before.cpu.stat.aggregate)
}

pure interface_rate(
  current_observation: Observation,
  current: network.NetworkInterface?,
  previous: Observation?,
  receive: Bool,
) -> rates.Rate {
  if current == null {
    return rate(rates.Unavailable)
  }

  if previous == null {
    return rate(rates.Warming)
  }

  let before = previous ?? current_observation
  if current_observation.at_ns <= before.at_ns {
    return rate(rates.Invalid)
  }

  if let prior = observed_interface(before.network) {
    let now = current
    if prior.name != now.name or prior.ifindex == null or now.ifindex == null or prior.ifindex != now.ifindex {
      return rate(rates.Warming)
    }

    let now_bytes = if receive { now.rx_bytes } else { now.tx_bytes }
    let prior_bytes = if receive { prior.rx_bytes } else { prior.tx_bytes }
    return rates.counter_rate(now_bytes, prior_bytes, elapsed_ns: current_observation.at_ns - before.at_ns)
  }

  rate(rates.Warming)
}

const PROCESS_METADATA_TTL_NS = 5000000000
type UidResult = {uid: Int?, state: MetadataState, entry: ProcessMetadata?}
type ProcessSortKey = {available: Int, negative_rate_tenths: Int, pid: Int}

pure process_index(previous: Observation?) -> Map[Int, processes.Process] {
  var rows: Map[Int, processes.Process] = {}
  if let before = previous {
    for row in before.processes.processes {
      rows = rows.set(row.pid, row)
    }
  }
  rows
}

pure core_index(previous: Observation?) -> Map[Int, syscpu.CpuCounters] {
  var rows: Map[Int, syscpu.CpuCounters] = {}
  if let before = previous {
    for core in before.cpu.stat.cores {
      rows = rows.set(core.id, core.counters)
    }
  }
  rows
}

pure core_rate(current: Observation, previous: Observation?, prior_cores: Map[Int, syscpu.CpuCounters], core: syscpu.CpuCore) -> rates.Rate {
  if previous == null {
    return rate(rates.Warming)
  }
  let before = previous ?? current
  if current.at_ns <= before.at_ns {
    return rate(rates.Invalid)
  }
  if let Ok(counters) = prior_cores.get(core.id) {
    rates.cpu_percent(core.counters, counters)
  } else {
    rate(rates.Warming)
  }
}

pure previous_process(index: Map[Int, processes.Process], current: processes.Process) -> processes.Process? {
  if let Ok(row) = index.get(current.pid) {
    if row.start_ticks == current.start_ticks { row } else { null }
  } else {
    null
  }
}

pure process_rate(current: Observation, previous: Observation?, prior_rows: Map[Int, processes.Process], row: processes.Process) -> rates.Rate {
  if row.cpu_ticks == null {
    return rate(rates.Unavailable)
  }

  if previous == null {
    return rate(rates.Warming)
  }

  let before = previous ?? current
  if current.at_ns <= before.at_ns {
    return rate(rates.Invalid)
  }

  if before.cpu.stat.aggregate == null or current.cpu.stat.aggregate == null {
    return rate(rates.Warming)
  }

  if let older = previous_process(prior_rows, row) {
    if older.cpu_ticks == null {
      return rate(rates.Warming)
    }

    let core_count = current.cpu.stat.cores.len()
    return rates.process_percent(
      row.cpu_ticks ?? 0,
      older.cpu_ticks,
      current.cpu.stat.aggregate,
      before.cpu.stat.aggregate,
      if core_count == 0 { 1 } else { core_count },
    )
  }

  rate(rates.Warming)
}

pure process_average_rate(current: Observation, row: processes.Process) -> rates.Rate {
  if row.cpu_ticks == null or current.cpu.uptime.seconds == null or current.clock_ticks_per_second <= 0 {
    return rate(rates.Unavailable)
  }
  let ticks_per_second = current.clock_ticks_per_second.float()
  let elapsed_seconds = (current.cpu.uptime.seconds ?? 0.0) - row.start_ticks.float() / ticks_per_second
  if elapsed_seconds <= 0.0 {
    return rate(rates.Invalid)
  }
  {
    state: rates.Observed,
    value: (row.cpu_ticks ?? 0).float() / ticks_per_second / elapsed_seconds * 100.0,
  }
}

pure cached_uid(row: processes.Process, current: Observation, cache: MetadataCache) -> UidResult {
  if let uid = row.uid {
    let entry = {start_ticks: row.start_ticks, command: row.command, uid: uid, observed_at_ns: current.at_ns}
    return {uid: uid, state: .Fresh, entry: entry}
  }

  if cache.source_id == current.source_id {
    if let Ok(prior) = cache.by_pid.get(row.pid) {
      let age = current.at_ns - prior.observed_at_ns
      if prior.start_ticks == row.start_ticks and prior.command == row.command and
          age >= 0 and age < PROCESS_METADATA_TTL_NS {
        return {uid: prior.uid, state: .Stale, entry: prior}
      }
    }
  }
  {uid: null, state: .Unavailable, entry: null}
}

pure process_sort_key(row: ProcessRow) -> ProcessSortKey {
  let observed = row.cpu_percent.state == rates.Observed and row.cpu_percent.value != null
  let negative_rate_tenths = if observed {
    if let Ok(tenth) = ((row.cpu_percent.value ?? 0.0) * 10.0).round() { -tenth } else { 0 }
  } else {
    0
  }
  {
    available: if observed { 0 } else { 1 },
    negative_rate_tenths: negative_rate_tenths,
    pid: row.pid,
  }
}

## Applies one source observation and retains only current process metadata.
export pure step_cached(current: Observation, previous: Observation?, metadata: MetadataCache) -> CachedStep {
  let baseline = same_source_baseline(current, previous)
  let prior_rows = process_index(baseline)
  let prior_cores = core_index(baseline)
  let cache = if metadata.source_id == current.source_id { metadata } else { {source_id: current.source_id, by_pid: {}} }
  let selected = observed_interface(current.network)
  var process_rows: List[ProcessRow] = []
  var next_metadata: Map[Int, ProcessMetadata] = {}
  var core_rows: List[CoreUsage] = []
  for core in current.cpu.stat.cores {
    core_rows += [{id: core.id, utilization: core_rate(current, baseline, prior_cores, core)}]
  }
  for process_row in current.processes.processes {
    let uid = cached_uid(process_row, current, cache)
    let row: ProcessRow = {
      pid: process_row.pid,
      parent_pid: process_row.parent_pid,
      start_ticks: process_row.start_ticks,
      thread_count: process_row.thread_count,
      uid: uid.uid,
      uid_state: uid.state,
      command: process_row.command,
      cpu_percent: process_rate(current, baseline, prior_rows, process_row),
      cpu_average_percent: process_average_rate(current, process_row),
      rss_bytes: process_row.rss_bytes,
    }
    process_rows += [row]
    if let entry = uid.entry {
      next_metadata = next_metadata.set(process_row.pid, entry)
    }
  }

  {
    value: {
      source_id: current.source_id,
      at_ns: current.at_ns,
      cpu_percent: cpu_interval(current, baseline),
      cpu_cores: core_rows,
      uptime: current.cpu.uptime,
      load: current.cpu.load,
      cpu_state: current.cpu.stat.state,
      memory: memory_panel(current.memory),
      network: {
        source_state: current.network.state,
        interface_name: if let chosen = selected { chosen.name } else { null },
        rx_bytes: if let chosen = selected { chosen.rx_bytes } else { null },
        tx_bytes: if let chosen = selected { chosen.tx_bytes } else { null },
        rx_bytes_per_second: interface_rate(current, selected, baseline, true),
        tx_bytes_per_second: interface_rate(current, selected, baseline, false),
      },
      process_state: current.processes.state,
      processes: process_rows |> sort-by { |row| process_sort_key(row) },
    },
    metadata: {source_id: current.source_id, by_pid: next_metadata},
  }
}

## Applies one source observation to the shared previous frame, if one exists.
export pure step(current: Observation, previous: Observation?) -> Model {
  step_cached(current, previous, {source_id: current.source_id, by_pid: {}}).value
}

## Starts a process metadata cache for one observation source.
export pure empty_metadata_cache(source_id: Str) -> MetadataCache {
  {source_id: source_id, by_pid: {}}
}
