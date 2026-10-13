##! Derives one monitor frame from raw rooted observations and a prior frame.
##!
##! The model owns interval semantics and stable process ranking; neither the
##! renderer nor replay decides whether a counter represents a reset or rate.
use btop_rates as rates
use btop_disks as disks
use btop_sensors as sensors
use sys_cpu as syscpu
use sys_mem as memory
use sys_net as network
use sys_proc as processes
use system_report as report
use system_report_live as system_live

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
  telemetry: system_live.MonitorSources,
  storage: disks.Source,
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
  cpu_percent_per_core: rates.Rate,
  cpu_average_percent: rates.Rate,
  rss_bytes: Int?,
}
type ProcessUpdate = {row: ProcessRow, metadata: ProcessMetadata?}

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
type MetadataIndexEntry = {pid: Int, entry: ProcessMetadata}
## Bounds metadata reuse to one source session and the current process inventory.
export type MetadataCache = {source_id: Str, by_pid: Map[Int, ProcessMetadata]}
## Associates a current process with the same PID lifetime from the prior sample.
export type ProcessMatch = {current: processes.Process, previous: processes.Process?}

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

## Keeps one interface's counters and interval byte rates for display selection.
export type NetworkMetric = {
  name: Str,
  ifindex: Int?,
  rx_bytes: Int,
  tx_bytes: Int,
  rx_bytes_per_second: rates.Rate,
  tx_bytes_per_second: rates.Rate,
}

## Keeps the deterministic default interface and every measured interface.
export type NetworkPanel = {
  source_state: report.ObservationState,
  interface_name: Str?,
  rx_bytes: Int?,
  tx_bytes: Int?,
  rx_bytes_per_second: rates.Rate,
  tx_bytes_per_second: rates.Rate,
  interfaces: List[NetworkMetric],
}

## Holds the display-ready values for one sampling instant.
export type Model = {
  source_id: Str,
  at_ns: Int,
  cpu_percent: rates.Rate,
  cpu_cores: List[CoreUsage],
  telemetry: sensors.Telemetry,
  uptime: syscpu.Uptime,
  load: syscpu.LoadAverage,
  cpu_state: report.ObservationState,
  memory: MemoryPanel,
  network: NetworkPanel,
  process_state: report.ObservationState,
  processes: List[ProcessRow],
  storage: disks.Panel,
}

## Returns the display model and the next bounded metadata cache together.
export type CachedStep = {value: Model, metadata: MetadataCache}

pure rate(state: rates.RateState) -> rates.Rate {
  {state: state, value: null}
}

pure same_source_baseline(current: Observation, previous: Observation?) -> Observation? {
  return null when previous == null

  let before = previous ?? current
  return null when before.source_id != current.source_id

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

pure cpu_interval(current: Observation, previous: Observation?, gap: Bool) -> rates.Rate {
  if current.cpu.stat.aggregate == null {
    return rate(if current.cpu.stat.state == report.Observed { rates.Invalid } else { rates.Unavailable })
  }

  return rate(rates.Warming) when previous == null

  let before = previous ?? current
  return rate(rates.Invalid) when current.at_ns <= before.at_ns

  return rate(rates.Warming) when before.cpu.stat.aggregate == null
  return rate(rates.Gap) when gap

  rates.cpu_percent(current.cpu.stat.aggregate, before.cpu.stat.aggregate)
}

pure prior_interface(
  source: network.NetworkSource,
  current: network.NetworkInterface,
) -> network.NetworkInterface? {
  for candidate in source.interfaces {
    return candidate when candidate.name == current.name
  }
  null
}

pure interface_rate(
  current_observation: Observation,
  current: network.NetworkInterface,
  previous: Observation?,
  receive: Bool,
  gap: Bool,
) -> rates.Rate {
  return rate(rates.Warming) when previous == null

  let before = previous ?? current_observation
  return rate(rates.Invalid) when current_observation.at_ns <= before.at_ns

  if let prior = prior_interface(before.network, current) {
    if prior.ifindex == null or current.ifindex == null or prior.ifindex != current.ifindex {
      return rate(rates.Warming)
    }
    return rate(rates.Gap) when gap

    let now_bytes = if receive { current.rx_bytes } else { current.tx_bytes }
    let prior_bytes = if receive { prior.rx_bytes } else { prior.tx_bytes }
    return rates.counter_rate(now_bytes, prior_bytes, elapsed_ns: current_observation.at_ns - before.at_ns)
  }

  rate(rates.Warming)
}

pure network_metrics(
  current: Observation,
  previous: Observation?,
  gap: Bool,
) -> List[NetworkMetric] {
  collect {
    for interface in current.network.interfaces {
      yield {
        name: interface.name,
        ifindex: interface.ifindex,
        rx_bytes: interface.rx_bytes,
        tx_bytes: interface.tx_bytes,
        rx_bytes_per_second: interface_rate(current, interface, previous, true, gap),
        tx_bytes_per_second: interface_rate(current, interface, previous, false, gap),
      }
    }
  }
}

pure find_network_metric(values: List[NetworkMetric], name: Str?) -> NetworkMetric? {
  return null when name == null
  for value in values {
    return value when value.name == (name ?? "")
  }
  null
}

## Selects an already-derived network interface without changing its history.
export pure network_metric(value: NetworkPanel, name: Str?) -> NetworkMetric? {
  find_network_metric(value.interfaces, if name == null { value.interface_name } else { name })
}

const PROCESS_METADATA_TTL_NS = 5000000000
type UidResult = {uid: Int?, state: MetadataState, entry: ProcessMetadata?}

pure core_index(previous: Observation?) -> Map[Int, syscpu.CpuCounters] {
  if let before = previous {
    return {core.id: core.counters for core in before.cpu.stat.cores}
  }
  {}
}

pure core_rate(
  current: Observation,
  previous: Observation?,
  prior_cores: Map[Int, syscpu.CpuCounters],
  core: syscpu.CpuCore,
  gap: Bool,
) -> rates.Rate {
  return rate(rates.Warming) when previous == null
  let before = previous ?? current
  return rate(rates.Invalid) when current.at_ns <= before.at_ns
  if let Ok(counters) = prior_cores.get(core.id) {
    if gap { rate(rates.Gap) } else { rates.cpu_percent(core.counters, counters) }
  } else {
    rate(rates.Warming)
  }
}

## Joins each current process against one indexed prior PID and lifetime.
export pure process_join(
  current: List[processes.Process],
  previous: List[processes.Process],
) -> List[ProcessMatch] {
  let previous_by_pid = {row.pid: row for row in previous}
  collect {
    for row in current {
      let candidate: processes.Process? = if row.pid in previous_by_pid { previous_by_pid[row.pid] } else { null }
      let matching_prior: processes.Process? = if let prior = candidate {
        if prior.start_ticks == row.start_ticks { prior } else { null }
      } else {
        null
      }
      yield {
        current: row,
        previous: matching_prior,
      }
    }
  }
}

pure process_rate(
  current: Observation,
  previous: Observation?,
  previous_process: processes.Process?,
  row: processes.Process,
  gap: Bool,
  per_core: Bool = false,
) -> rates.Rate {
  return rate(rates.Unavailable) when row.cpu_ticks == null

  return rate(rates.Warming) when previous == null

  let before = previous ?? current
  return rate(rates.Invalid) when current.at_ns <= before.at_ns

  if before.cpu.stat.aggregate == null or current.cpu.stat.aggregate == null {
    return rate(rates.Warming)
  }

  if let older = previous_process {
    return rate(rates.Warming) when older.cpu_ticks == null
    return rate(rates.Gap) when gap

    let core_count = current.cpu.stat.cores.len()
    return rates.process_percent(
      row.cpu_ticks ?? 0,
      older.cpu_ticks,
      current.cpu.stat.aggregate,
      before.cpu.stat.aggregate,
      if core_count == 0 { 1 } else { core_count },
      per_core: per_core,
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
  return rate(rates.Invalid) when elapsed_seconds <= 0.0
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

pure process_sort_rate_tenths(row: ProcessRow) -> Int {
  if row.cpu_percent.state == rates.Observed and row.cpu_percent.value != null {
    if let Ok(tenth) = ((row.cpu_percent.value ?? 0.0) * 10.0).round() { -tenth } else { 0 }
  } else {
    0
  }
}

pure process_sort_availability(row: ProcessRow) -> Int {
  if row.cpu_percent.state == rates.Observed and row.cpu_percent.value != null { 0 } else { 1 }
}

## Applies one source observation and retains only current process metadata.
export pure step_cached(
  current: Observation,
  previous: Observation?,
  metadata: MetadataCache,
  gap: Bool = false,
) -> CachedStep {
  let baseline = same_source_baseline(current, previous)
  let prior_cores = core_index(baseline)
  let before_processes: List[processes.Process] = if let before = baseline {
    before.processes.processes
  } else {
    []
  }
  let process_matches = process_join(current.processes.processes, before_processes)
  let cache = if metadata.source_id == current.source_id { metadata } else { {source_id: current.source_id, by_pid: {}} }
  let storage_previous: disks.Source? = if let before = baseline { before.storage } else { null }
  let storage_previous_at: Int? = if let before = baseline { before.at_ns } else { null }
  let selected = observed_interface(current.network)
  let interface_metrics = network_metrics(current, baseline, gap)
  let selected_metric = find_network_metric(
    interface_metrics,
    if let interface = selected { interface.name } else { null },
  )
  let core_rows: List[CoreUsage] = collect {
    for core in current.cpu.stat.cores {
      yield {id: core.id, utilization: core_rate(current, baseline, prior_cores, core, gap)}
    }
  }
  let process_updates: List[ProcessUpdate] = collect {
    for matched in process_matches {
      let process_row = matched.current
      let uid = cached_uid(process_row, current, cache)
      yield {
        row: {
          pid: process_row.pid,
          parent_pid: process_row.parent_pid,
          start_ticks: process_row.start_ticks,
          thread_count: process_row.thread_count,
          uid: uid.uid,
          uid_state: uid.state,
          command: process_row.command,
          cpu_percent: process_rate(current, baseline, matched.previous, process_row, gap),
          cpu_percent_per_core: process_rate(current, baseline, matched.previous, process_row, gap, per_core: true),
          cpu_average_percent: process_average_rate(current, process_row),
          rss_bytes: process_row.rss_bytes,
        },
        metadata: uid.entry,
      }
    }
  }
  let process_rows: List[ProcessRow] = [update.row for update in process_updates]
  let next_metadata_entries: List[MetadataIndexEntry] = collect {
    for update in process_updates {
      if let entry = update.metadata {
        yield {pid: update.row.pid, entry: entry}
      }
    }
  }
  let next_metadata: Map[Int, ProcessMetadata] = {
    item.pid: item.entry
    for item in next_metadata_entries
  }

  {
    value: {
      source_id: current.source_id,
      at_ns: current.at_ns,
      cpu_percent: cpu_interval(current, baseline, gap),
      cpu_cores: core_rows,
      telemetry: sensors.select(current.telemetry, current.cpu.stat.cores |> map .id),
      uptime: current.cpu.uptime,
      load: current.cpu.load,
      cpu_state: current.cpu.stat.state,
      memory: memory_panel(current.memory),
      network: {
        source_state: current.network.state,
        interface_name: if let chosen = selected { chosen.name } else { null },
        rx_bytes: if let chosen = selected { chosen.rx_bytes } else { null },
        tx_bytes: if let chosen = selected { chosen.tx_bytes } else { null },
        rx_bytes_per_second: if let chosen = selected_metric { chosen.rx_bytes_per_second } else { rate(rates.Unavailable) },
        tx_bytes_per_second: if let chosen = selected_metric { chosen.tx_bytes_per_second } else { rate(rates.Unavailable) },
        interfaces: interface_metrics,
      },
      process_state: current.processes.state,
      processes: process_rows
        |> sort-by .pid
        |> sort-by { |row| process_sort_rate_tenths(row) }
        |> sort-by { |row| process_sort_availability(row) },
      storage: disks.step(current.storage, storage_previous, current.at_ns, storage_previous_at, gap),
    },
    metadata: {
      source_id: current.source_id,
      by_pid: next_metadata,
    },
  }
}

## Applies one source observation to the shared previous frame, if one exists.
export pure step(current: Observation, previous: Observation?, gap: Bool = false) -> Model {
  step_cached(current, previous, {source_id: current.source_id, by_pid: {}}, gap).value
}

pure counter_wire_text(value: Int?) -> Str? {
  if let counter = value { f"{counter}" } else { null }
}

pure process_wire(row: ProcessRow) -> Any {
  {
    pid: row.pid,
    parent_pid: row.parent_pid,
    start_ticks: f"{row.start_ticks}",
    thread_count: row.thread_count,
    uid: row.uid,
    uid_state: row.uid_state,
    command: row.command,
    cpu_percent: row.cpu_percent,
    cpu_percent_per_core: row.cpu_percent_per_core,
    cpu_average_percent: row.cpu_average_percent,
    rss_bytes: row.rss_bytes,
  }
}

pure network_wire(value: NetworkPanel) -> Any {
  {
    source_state: value.source_state,
    interface_name: value.interface_name,
    rx_bytes: counter_wire_text(value.rx_bytes),
    tx_bytes: counter_wire_text(value.tx_bytes),
    rx_bytes_per_second: value.rx_bytes_per_second,
    tx_bytes_per_second: value.tx_bytes_per_second,
    interfaces: [
      {
        name: item.name,
        ifindex: item.ifindex,
        rx_bytes: f"{item.rx_bytes}",
        tx_bytes: f"{item.tx_bytes}",
        rx_bytes_per_second: item.rx_bytes_per_second,
        tx_bytes_per_second: item.tx_bytes_per_second,
      }
      for item in value.interfaces
    ],
  }
}

pure disk_wire(value: disks.Device) -> Any {
  {
    identity: value.identity,
    identity_state: value.identity_state,
    name: value.name,
    major: value.major,
    minor: value.minor,
    mount_targets: value.mount_targets,
    filesystem: value.filesystem,
    capacity_state: value.capacity_state,
    capacity_age_ns: value.capacity_age_ns,
    total_bytes: counter_wire_text(value.total_bytes),
    used_bytes: counter_wire_text(value.used_bytes),
    available_bytes: counter_wire_text(value.available_bytes),
    read_sectors: counter_wire_text(value.read_sectors),
    read_state: value.read_state,
    read_bytes_per_second: value.read_bytes_per_second,
    write_sectors: counter_wire_text(value.write_sectors),
    write_state: value.write_state,
    write_bytes_per_second: value.write_bytes_per_second,
    in_flight: value.in_flight,
    in_flight_state: value.in_flight_state,
  }
}

pure storage_wire(value: disks.Panel) -> Any {
  {
    block_state: value.block_state,
    mount_state: value.mount_state,
    devices: [disk_wire(device) for device in value.devices],
  }
}

## Encodes raw counters as canonical decimal strings while keeping typed arithmetic in Int.
export pure encode_wire_json(value: Model) -> Result[Str, Error] {
  let wire: Any = {
    source_id: value.source_id,
    at_ns: value.at_ns,
    cpu_percent: value.cpu_percent,
    cpu_cores: value.cpu_cores,
    telemetry: value.telemetry,
    uptime: value.uptime,
    load: value.load,
    cpu_state: value.cpu_state,
    memory: value.memory,
    network: network_wire(value.network),
    process_state: value.process_state,
    processes: [process_wire(row) for row in value.processes],
    storage: storage_wire(value.storage),
  }
  json.encode(wire, pretty: false)
}

## Starts a process metadata cache for one observation source.
export pure empty_metadata_cache(source_id: Str) -> MetadataCache {
  {source_id: source_id, by_pid: {}}
}
