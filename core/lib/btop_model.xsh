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
  uid: Int?,
  command: Str,
  cpu_percent: rates.Rate,
  rss_bytes: Int?,
}

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
  uptime: syscpu.Uptime,
  load: syscpu.LoadAverage,
  cpu_state: report.ObservationState,
  memory: MemoryPanel,
  network: NetworkPanel,
  process_state: report.ObservationState,
  processes: List[ProcessRow],
}

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
    if prior.name != now.name {
      return rate(rates.Warming)
    }

    let now_bytes = if receive { now.rx_bytes } else { now.tx_bytes }
    let prior_bytes = if receive { prior.rx_bytes } else { prior.tx_bytes }
    return rates.counter_rate(now_bytes, prior_bytes, elapsed_ns: current_observation.at_ns - before.at_ns)
  }

  rate(rates.Warming)
}

pure previous_process(previous: Observation, current: processes.Process) -> processes.Process? {
  let rows = previous.processes.processes
    |> where .pid == current.pid and .start_ticks == current.start_ticks
  if rows.is_empty() {
    return null
  }

  rows[0]
}

pure process_rate(current: Observation, previous: Observation?, row: processes.Process) -> rates.Rate {
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

  if let older = previous_process(before, row) {
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

pure before(left: ProcessRow, right: ProcessRow) -> Bool {
  if left.cpu_percent.state == rates.Observed and right.cpu_percent.state != rates.Observed {
    return true
  }
  if left.cpu_percent.state != rates.Observed and right.cpu_percent.state == rates.Observed {
    return false
  }
  if left.cpu_percent.state == rates.Observed and right.cpu_percent.state == rates.Observed {
    let left_value = left.cpu_percent.value ?? 0.0
    let right_value = right.cpu_percent.value ?? 0.0
    if left_value != right_value {
      return left_value > right_value
    }
  }
  left.pid < right.pid
}

pure insert_process(rows: List[ProcessRow], row: ProcessRow) -> List[ProcessRow] {
  var result: List[ProcessRow] = []
  var inserted = false
  for current in rows {
    if ! inserted and before(row, current) {
      result += [row]
      inserted = true
    }
    result += [current]
  }
  if ! inserted {
    result += [row]
  }
  result
}

## Applies one source observation to the shared previous frame, if one exists.
export pure step(current: Observation, previous: Observation?) -> Model {
  let baseline = same_source_baseline(current, previous)
  let selected = observed_interface(current.network)
  var process_rows: List[ProcessRow] = []
  for process_row in current.processes.processes {
    let row: ProcessRow = {
      pid: process_row.pid,
      uid: process_row.uid,
      command: process_row.command,
      cpu_percent: process_rate(current, baseline, process_row),
      rss_bytes: process_row.rss_bytes,
    }
    process_rows = insert_process(process_rows, row)
  }

  {
    source_id: current.source_id,
    at_ns: current.at_ns,
    cpu_percent: cpu_interval(current, baseline),
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
    processes: process_rows,
  }
}
