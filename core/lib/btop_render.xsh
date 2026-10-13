##! Builds one complete semantic monitor frame without terminal side effects.
##!
##! G4a fixes the default panel geometry and buffers every row before output.
use btop_model as model
use btop_color as color
use btop_rates as rates
use btop_graph as graph
use btop_graphs as graph_series
use btop_sensors as sensors
use btop_surface as surface
use btop_terminal as terminal
use btop_ui as ui
use btop_defaults as defaults
use btop_disks as disks
use sys_proc as processes
use sys_cpu as syscpu
use system_report as report
use tui

## Holds the full visible row set and whether the terminal can show the panels.
export type Screen = {
  width: Int,
  height: Int,
  too_small: Bool,
  lines: List[Str],
  row_roles: List[color.Role?],
}

pure repeated(character: Str, count: Int) -> Str {
  var text = ""
  for _ in range(count) {
    text += character
  }
  text
}

pure safe_line(value: Str) -> Str {
  tui.safe_single_line(value)
}

pure fit(value: Str, width: Int) -> Str {
  return "" when width <= 0
  tui.cell_right_pad(tui.cell_clip(safe_line(value), width), width)
}

pure top_border(width: Int, title: Str) -> Str {
  let inner_width = width - 2
  let label = f" {title} "
  let spare = inner_width - label.count_chars()
  let left = if spare < 0 { 0 } else { spare / 2 }
  let right = if spare < 0 { 0 } else { spare - left }
  f"╭{repeated("─", left)}{label}{repeated("─", right)}╮"
}

pure bottom_border(width: Int) -> Str {
  f"╰{repeated("─", width - 2)}╯"
}

pure panel(width: Int, height: Int, title: Str, content: List[Str]) -> List[Str] {
  var rows = [top_border(width, title)]
  for index in range(height - 2) {
    let text = if index < content.len() { content[index] } else { "" }
    rows += [f"│{fit(text, width - 2)}│"]
  }
  rows += [bottom_border(width)]
  rows
}

pure split_panel(
  width: Int,
  height: Int,
  title: Str,
  left_content: List[Str],
  right_content: List[Str],
) -> List[Str] {
  let left_width = (width - 3) / 2
  let right_width = width - 3 - left_width
  var rows = [top_border(width, title)]
  for index in range(height - 2) {
    let left = if index < left_content.len() { left_content[index] } else { "" }
    let right = if index < right_content.len() { right_content[index] } else { "" }
    rows += [f"│{fit(left, left_width)}│{fit(right, right_width)}│"]
  }
  rows += [bottom_border(width)]
  rows
}

pure state_text(state: report.ObservationState) -> Str {
  match state {
    report.Absent => "absent"
    report.Malformed => "malformed"
    report.Truncated => "truncated"
    report.PermissionDenied => "permission denied"
    report.ReadFailure => "read failed"
    else => "unavailable"
  }
}

pure rate_text(value: rates.Rate) -> Str {
  if value.state == .Observed and value.value != null {
    return f"{(value.value).format(precision: 1)}%"
  }

  match value.state {
    rates.Warming => "warming"
    rates.Reset => "reset"
    rates.Invalid => "invalid"
    rates.Gap => "gap"
    else => "unavailable"
  }
}

pure byte_rate_text(value: rates.Rate) -> Str {
  if value.state == .Observed and value.value != null {
    let amount = value.value
    return f"{(amount / 1048576.0).format(precision: 2)} MiB/s" when amount >= 1048576.0
    return f"{(amount / 1024.0).format(precision: 2)} KiB/s" when amount >= 1024.0
    return f"{amount.format(precision: 2)} B/s"
  }

  match value.state {
    rates.Warming => "warming"
    rates.Reset => "reset"
    rates.Invalid => "invalid"
    rates.Gap => "gap"
    else => "unavailable"
  }
}

pure bytes_text(value: Int?) -> Str {
  return "unavailable" when value == null
  let amount = value
  if amount >= 1073741824 {
    return f"{(amount.float() / 1073741824.0).format(precision: 2)} GiB"
  }
  return f"{(amount.float() / 1048576.0).format(precision: 2)} MiB" when amount >= 1048576
  return f"{(amount.float() / 1024.0).format(precision: 2)} KiB" when amount >= 1024
  f"{amount} B"
}

pure field_text(value: Int?, state: report.ObservationState) -> Str {
  return state_text(state) when value == null
  bytes_text(value)
}

pure counter_text(value: rates.ByteValue) -> Str {
  guard value.value != null else {
    return match value.state {
      rates.Invalid => "invalid"
      rates.Reset => "reset"
      else => "unavailable"
    }
  }
  bytes_text(value.value)
}

pure uptime_text(value: syscpu.Uptime) -> Str {
  return state_text(value.state) when value.seconds == null
  f"{(value.seconds).format(precision: 1)} s"
}

pure load_text(value: syscpu.LoadAverage) -> Str {
  if value.state != .Observed or value.values.len() < 3 {
    return state_text(value.state)
  }
  f"{value.values[0].format(precision: 2)} {value.values[1].format(precision: 2)} {value.values[2].format(precision: 2)}"
}

pure temperature_text(value: sensors.CpuTemperature) -> Str {
  return f"Temperature {state_text(value.state)}" when value.package_millidegrees == null
  let display = sensors.temperature_display(value.package_millidegrees, "celsius")
  if let temperature = display {
    f"Temperature {temperature.value}{temperature.unit}"
  } else {
    f"Temperature {state_text(value.state)}"
  }
}

pure core_temperature_text(value: sensors.CpuTemperature) -> Str {
  return "Core temperatures unavailable" when value.cores.is_empty()
  let readings: List[Str] = collect {
    for core in value.cores {
      let display = sensors.temperature_display(core.millidegrees, "celsius")
      if let temperature = display {
        yield f"C{core.id} {temperature.value}{temperature.unit}"
      }
    }
  }
  f"Cores {readings.join(" ")}"
}

pure frequency_text(value: sensors.CpuFrequency) -> Str {
  return f"Frequency {state_text(value.state)}" when value.state != .Observed
  if value.minimum_mhz != null and value.maximum_mhz != null {
    return f"Frequency {sensors.format_frequency_mhz(value.minimum_mhz)} - {sensors.format_frequency_mhz(value.maximum_mhz)}"
  }
  if let mhz = value.value_mhz {
    return f"Frequency {sensors.format_frequency_mhz(mhz)}"
  }
  "Frequency unavailable"
}

pure battery_text(value: sensors.Battery) -> Str {
  return f"Battery {state_text(value.state)}" when value.state != .Observed
  let percent = if value.capacity_percent == null { "?" } else { f"{value.capacity_percent}%" }
  let power = if value.power_watts == null { "" } else { f" {(value.power_watts).format(precision: 2)} W" }
  f"Battery {value.name ?? "?"} {percent} {value.status ?? "unknown"}{power}"
}

pure process_text(
  value: model.Model,
  entry: ui.VisibleRow,
  state: ui.State,
  local_display: List[processes.LocalProcessDisplay],
) -> Str {
  let row = entry.row
  let account = if let display = ui.local_display(row, local_display) {
    display.user_name
  } else if row.uid == null {
    "?"
  } else {
    f"{row.uid}"
  }
  let user_label = if row.uid_state == .Stale { f"~{account}" } else { account }
  let command = if let display = ui.local_display(row, local_display) {
    display.command_line ?? row.command
  } else {
    row.command
  }
  let process_cpu = if state.aggregate_children {
    if state.process_cpu_per_core { entry.cpu_total_per_core } else { entry.cpu_total }
  } else if state.process_cpu_per_core {
    row.cpu_percent_per_core
  } else {
    row.cpu_percent
  }
  let cpu_text = rate_text(process_cpu)
  let rss_bytes = if state.aggregate_children { entry.rss_total_bytes } else { row.rss_bytes }
  let rss = if state.process_memory_bytes {
    bytes_text(rss_bytes)
  } else if rss_bytes != null and value.memory.total_bytes != null and (value.memory.total_bytes) > 0 {
    f"{((rss_bytes).float() * 100.0 / (value.memory.total_bytes).float()).format(precision: 1)}%"
  } else {
    "unavailable"
  }
  let indent = repeated("  ", entry.depth)
  let tree_marker = if ! state.tree_view {
    ""
  } else if entry.has_children {
    if entry.collapsed { "[+] " } else { "[-] " }
  } else if entry.depth > 0 {
    "└ "
  } else {
    ""
  }
  f"{indent}{tree_marker}{row.pid} {user_label} {cpu_text} {rss} {command}"
}

pure memory_percent_text(value: model.Model) -> Str {
  if value.memory.total_bytes != null and (value.memory.total_bytes) > 0 and value.memory.used_bytes.value != null {
    let percent = (value.memory.used_bytes.value).float() * 100.0 / (value.memory.total_bytes).float()
    return f"{percent.format(precision: 1)}%"
  }
  match value.memory.used_bytes.state {
    rates.Warming => "warming"
    rates.Reset => "reset"
    rates.Gap => "gap"
    rates.Invalid => "invalid"
    else => "unavailable"
  }
}

pure history_rows(
  history: graph.History,
  capabilities: terminal.Capabilities,
  width: Int,
  height: Int,
  inverted: Bool,
  maximum: Float?,
  minimum: Float = 0.0,
) -> Result[List[Str], Error] {
  let frame = graph.project(history, capabilities, width, height, inverted, maximum, minimum)?
  Ok(frame.rows)
}

pure process_content(
  value: model.Model,
  state: ui.State,
  capacity: Int,
  width: Int,
  local_display: List[processes.LocalProcessDisplay],
) -> List[Str] {
  let sort_name = match state.sort {
    ui.Pid => "PID"
    ui.Name => "NAME"
    ui.Cmd => "COMMAND"
    ui.ThreadCount => "THREADS"
    ui.UserName => "USER"
    ui.Memory => "RSS"
    ui.CpuDirect => "CPU direct"
    ui.CpuLazy => "CPU lazy"
  }
  let direction = match state.sort {
    ui.Pid | .ThreadCount | .Memory | .CpuDirect | .CpuLazy => if state.reverse { "ascending" } else { "descending" }
    ui.Name | .Cmd | .UserName => if state.reverse { "descending" } else { "ascending" }
  }
  let sort_line = f"Sort {sort_name} {direction} | r reverse"
  let filter_line = if state.editing_filter {
    f"Filter: {state.filter_draft}_ | Enter accept, Esc cancel"
  } else if state.filter != "" {
    f"Filter: {state.filter} | f edit, Del clear"
  } else {
    f"{sort_line} | f filter"
  }
  let cpu_column = if state.process_cpu_per_core { "CPU/core%" } else { "CPU%" }
  var lines = [f"PID USER {cpu_column} RSS COMMAND", filter_line]
  if value.process_state != .Observed {
    lines += [f"Processes {state_text(value.process_state)}"]
  } else if value.processes.is_empty() {
    lines += ["No visible processes"]
  } else {
    let rows = ui.visible_tree_rows(value, state, local_display)
    if rows.is_empty() {
      lines += ["No matching processes"]
    } else {
      for index in range(capacity) {
        let at = state.scroll + index
        if at < rows.len() {
          let row = rows[at]
          lines += [surface.selected_row(process_text(value, row, state, local_display), ui.is_selected(state, row.row), width)]
        } else {
          lines += [""]
        }
      }
    }
  }
  lines
}

## Returns the process rows that fit below the pinned default CPU panel.
export pure process_capacity(height: Int) -> Int {
  return 0 when height < 24
  let cpu_height = if (height * 32 + 99) / 100 < 8 { 8 } else { (height * 32 + 99) / 100 }
  let process_height = height - cpu_height
  if process_height <= 4 { 0 } else { process_height - 4 }
}

pure default_capabilities(width: Int, height: Int) -> terminal.Capabilities {
  terminal.detect({
    term: "xterm-256color",
    colorterm: "",
    no_color: false,
    lc_all: "C.UTF-8",
    lc_ctype: "",
    lang: "",
    rows: height,
    cols: width,
  })
}

pure cpu_panel_height(height: Int) -> Int {
  let preferred = (height * 32 + 99) / 100
  if preferred < 8 { 8 } else { preferred }
}

pure memory_panel_width(width: Int) -> Int {
  (width * 55 + 50) / 100
}

pure cpu_content(
  value: model.Model,
  histories: graph_series.Histories,
  capabilities: terminal.Capabilities,
  width: Int,
  panel_height: Int,
) -> Result[List[Str], Error] {
  let graph_space = panel_height - 5
  let cpu_graph_height = (graph_space * 2 + 1) / 3
  let temperature_graph_height = graph_space - cpu_graph_height
  let cpu_graph = history_rows(histories.cpu, capabilities, width, cpu_graph_height, false, 100.0)?
  let temperature_graph = history_rows(
    histories.temperature,
    capabilities,
    width,
    temperature_graph_height,
    true,
    null,
    -20.0,
  )?
  let summary = f"Usage {rate_text(value.cpu_percent)} | Uptime {uptime_text(value.uptime)} | Load {load_text(value.load)}"
  let sensors = f"{temperature_text(value.telemetry.temperature)} | {frequency_text(value.telemetry.frequency)}"
  let details = f"{core_temperature_text(value.telemetry.temperature)} | {battery_text(value.telemetry.battery)}"
  Ok([summary, @cpu_graph, @temperature_graph, sensors, details])
}

pure memory_panel_content(
  value: model.Model,
  history: graph.History,
  capabilities: terminal.Capabilities,
  width: Int,
) -> Result[List[Str], Error] {
  let graph_width = if width > 0 { width } else { 1 }
  let used_graph = history_rows(history, capabilities, graph_width, 2, false, 100.0)?
  let swap = if value.memory.swap_total_state == .Absent and value.memory.swap_free_state == .Absent {
    "Swap absent"
  } else {
    f"Swap {counter_text(value.memory.swap_used_bytes)}"
  }
  Ok([f"Used {memory_percent_text(value)}", @used_graph, f"Total {field_text(value.memory.total_bytes, value.memory.total_state)}", f"Available {field_text(value.memory.available_bytes, value.memory.available_state)}", swap])
}

pure network_panel_content(
  value: model.Model,
  state: ui.State,
  histories: graph_series.Histories,
  capabilities: terminal.Capabilities,
  width: Int,
) -> Result[List[Str], Error] {
  let graph_width = if width > 0 { width } else { 1 }
  let receive_max = network_graph_maximum(histories.network_receive)
  let transmit_max = network_graph_maximum(histories.network_transmit)
  let maxima = if state.network_auto {
    if state.network_sync {
      let shared = if receive_max > transmit_max { receive_max } else { transmit_max }
      {receive: shared, transmit: shared}
    } else {
      {receive: receive_max, transmit: transmit_max}
    }
  } else {
    {
      receive: defaults.network_download_mibits().float() * 1048576.0 / 8.0,
      transmit: defaults.network_upload_mibits().float() * 1048576.0 / 8.0,
    }
  }
  let receive = history_rows(histories.network_receive, capabilities, graph_width, 1, false, maxima.receive)?
  let transmit = history_rows(histories.network_transmit, capabilities, graph_width, 1, true, maxima.transmit)?
  let selected = ui.selected_network_interface(state, value)
  let interface = if selected == null {
    f"Interface {state_text(value.network.source_state)}"
  } else {
    f"Interface {(selected ?? value.network.interfaces[0]).name}"
  }
  let scaling = f"Auto {if state.network_auto {"on"} else {"off"}} Sync {if state.network_sync {"on"} else {"off"}}"
  let rx_rate = if let current = selected { current.rx_bytes_per_second } else { {state: rates.Unavailable, value: null} }
  let tx_rate = if let current = selected { current.tx_bytes_per_second } else { {state: rates.Unavailable, value: null} }
  let rx_total = ui.network_total_bytes(state, selected, true)
  let tx_total = ui.network_total_bytes(state, selected, false)
  Ok([
    f"{interface} | {scaling}",
    f"RX {byte_rate_text(rx_rate)}  {bytes_text(rx_total)} total",
    receive[0],
    f"TX {byte_rate_text(tx_rate)}  {bytes_text(tx_total)} total",
    transmit[0],
  ])
}

pure network_graph_maximum(history: graph.History) -> Float {
  var maximum = 0.0
  for sample in history.samples {
    if sample.state == .Observed {
      if let value = sample.value {
        if value > maximum { maximum = value }
      }
    }
  }
  if maximum < 10240.0 { 10240.0 } else { maximum * 1.3 }
}

pure compact_bytes(value: Int?) -> Str {
  return "?" when value == null
  let amount = (value).float()
  return f"{(amount / 1073741824.0).format(precision: 0)}G" when amount >= 1073741824.0
  return f"{(amount / 1048576.0).format(precision: 0)}M" when amount >= 1048576.0
  return f"{(amount / 1024.0).format(precision: 0)}K" when amount >= 1024.0
  f"{value}B"
}

pure compact_rate(value: rates.Rate) -> Str {
  if value.state == .Observed and value.value != null {
    let amount = value.value
    return f"{(amount / 1073741824.0).format(precision: 0)}G/s" when amount >= 1073741824.0
    return f"{(amount / 1048576.0).format(precision: 0)}M/s" when amount >= 1048576.0
    return f"{(amount / 1024.0).format(precision: 0)}K/s" when amount >= 1024.0
    return f"{amount.format(precision: 0)}B/s"
  }
  match value.state {
    rates.Warming => "warming"
    rates.Reset => "reset"
    rates.Gap => "gap"
    rates.Invalid => "invalid"
    else => "unavailable"
  }
}

pure device_line(device: disks.Device) -> Str {
  let name = device.name ?? f"{device.major}:{device.minor}"
  let mount = if device.mount_targets.is_empty() { "?" } else { device.mount_targets[0] }
  f"{name} {mount}"
}

pure device_usage_line(device: disks.Device) -> Str {
  let capacity = if device.capacity_state == .Observed and device.total_bytes != null and device.used_bytes != null {
    f"{compact_bytes(device.used_bytes)}/{compact_bytes(device.total_bytes)}"
  } else {
    state_text(device.capacity_state)
  }
  f"{capacity} R{compact_rate(device.read_bytes_per_second)} W{compact_rate(device.write_bytes_per_second)}"
}

pure disk_panel_content(
  value: model.Model,
  state: ui.State,
  histories: graph_series.Histories,
  capabilities: terminal.Capabilities,
  width: Int,
  height: Int,
) -> Result[List[Str], Error] {
  return Ok(["Disks hidden"]) unless state.show_disks
  if value.storage.devices.is_empty() {
    if value.storage.mount_state == .Observed and value.storage.block_state == .Observed {
      return Ok(["No mounted block devices"])
    }
    let source_state = if value.storage.mount_state != .Observed { value.storage.mount_state } else { value.storage.block_state }
    return Ok([f"Storage {state_text(source_state)}"])
  }

  let graph_count = if state.disk_io_mode { 2 } else { 1 }
  let device_slots = (height - graph_count) / 2
  let shown_devices = if device_slots < value.storage.devices.len() { device_slots } else { value.storage.devices.len() }
  var lines = []
  for index in range(shown_devices) {
    let device = value.storage.devices[index]
    lines += [device_line(device), device_usage_line(device)]
  }
  if shown_devices < value.storage.devices.len() and lines.len() < height - graph_count {
    lines += [f"+{value.storage.devices.len() - shown_devices} more disks"]
  }

  if state.disk_io_mode {
    let read_graph = history_rows(histories.disk_read, capabilities, if width > 7 { width - 7 } else { 1 }, 1, false, null)?
    let write_graph = history_rows(histories.disk_write, capabilities, if width > 7 { width - 7 } else { 1 }, 1, true, null)?
    lines += [f"R {read_graph[0]}"]
    lines += [f"W {write_graph[0]}"]
  } else {
    let graph = history_rows(histories.disk_total, capabilities, if width > 3 { width - 3 } else { 1 }, 1, false, null)?
    lines += [f"IO {graph[0]}"]
  }
  Ok(lines)
}

## Renders a default process view at the injected window size.
export pure render(value: model.Model, width: Int, height: Int) -> Result[Screen, Error] {
  render_view(value, ui.initial(value, process_capacity(height)), width, height, [])
}

## Renders current selection, sort, filter, and scroll state without I/O.
export pure render_view(
  value: model.Model,
  state: ui.State,
  width: Int,
  height: Int,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> Result[Screen, Error] {
  render_view_with_graphs(value, state, width, height, local_display, graph_series.empty(), default_capabilities(width, height))
}

## Renders one fixed default panel layout from the retained series and captured capabilities.
export pure render_view_with_graphs(
  value: model.Model,
  state: ui.State,
  width: Int,
  height: Int,
  local_display: List[processes.LocalProcessDisplay],
  histories: graph_series.Histories,
  capabilities: terminal.Capabilities,
) -> Result[Screen, Error] {
  if width < 80 or height < 24 {
    return Ok({
      width: width,
      height: height,
      too_small: true,
      lines: [fit("btop: terminal too small (minimum 80x24)", width)],
      row_roles: [null],
    })
  }

  let cpu_height = cpu_panel_height(height)
  let lower_height = height - cpu_height
  let memory_height = height * 40 / 100
  let network_height = lower_height - memory_height
  let memory_width = memory_panel_width(width)
  let process_width = width - memory_width
  let cpu_content_rows = cpu_content(value, histories, capabilities, width - 2, cpu_height)?
  let cpu_rows = panel(width, cpu_height, "CPU", cpu_content_rows)
  let memory_left_width = (memory_width - 3) / 2
  let memory_left = memory_panel_content(value, histories.memory, capabilities, memory_left_width)?
  let memory_right_width = memory_width - 3 - memory_left_width
  let disk_content = disk_panel_content(value, state, histories, capabilities, memory_right_width, memory_height - 2)?
  let memory_rows = split_panel(memory_width, memory_height, "Memory / Disks", memory_left, disk_content)
  let network_content_rows = network_panel_content(value, state, histories, capabilities, memory_width - 2)?
  let network_rows = panel(memory_width, network_height, "Network", network_content_rows)
  let view = ui.resize(state, value, process_capacity(height))
  let process_rows = panel(
    process_width,
    lower_height,
    "Processes",
    process_content(value, view, process_capacity(height), process_width - 2, local_display),
  )
  var lines = cpu_rows
  var row_roles: List[color.Role?] = []
  for _ in range(cpu_height) { row_roles += [color.Cpu] }
  for index in range(lower_height) {
    let left = if index < memory_height {
      memory_rows[index]
    } else {
      network_rows[index - memory_height]
    }
    lines += [f"{left}{process_rows[index]}"]
    row_roles += [if index < memory_height { color.Memory } else { color.Network }]
  }
  Ok({width: width, height: height, too_small: false, lines: lines, row_roles: row_roles})
}
