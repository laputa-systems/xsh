##! Builds one complete ASCII monitor frame without terminal side effects.
##!
##! G1 keeps a fixed four-panel layout; it buffers all rows before the caller
##! writes them so a refresh never exposes a partially assembled screen.
use btop_model as model
use btop_rates as rates
use btop_surface as surface
use btop_ui as ui
use sys_proc as processes
use sys_cpu as syscpu
use system_report as report
use tui

## Holds the full visible row set and whether the terminal can show the panels.
export type Screen = {width: Int, height: Int, too_small: Bool, lines: List[Str]}

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
  if width <= 0 {
    return ""
  }
  tui.cell_right_pad(tui.cell_clip(safe_line(value), width), width)
}

pure panel(width: Int, height: Int, title: Str, content: List[Str]) -> List[Str] {
  let inner_width = width - 2
  let border = f"+{repeated("-", inner_width)}+"
  var rows = [border, f"|{fit(title, inner_width)}|"]
  for index in range(height - 3) {
    let text = if index < content.len() { content[index] } else { "" }
    rows += [f"|{fit(text, inner_width)}|"]
  }
  rows += [border]
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
  if value.state == rates.Observed and value.value != null {
    return f"{(value.value ?? 0.0).format(precision: 1)}%"
  }

  match value.state {
    rates.Warming => "warming"
    rates.Reset => "reset"
    rates.Invalid => "invalid"
    else => "unavailable"
  }
}

pure byte_rate_text(value: rates.Rate) -> Str {
  if value.state == rates.Observed and value.value != null {
    let amount = value.value ?? 0.0
    if amount >= 1048576.0 {
      return f"{(amount / 1048576.0).format(precision: 2)} MiB/s"
    }
    if amount >= 1024.0 {
      return f"{(amount / 1024.0).format(precision: 2)} KiB/s"
    }
    return f"{amount.format(precision: 2)} B/s"
  }

  match value.state {
    rates.Warming => "warming"
    rates.Reset => "reset"
    rates.Invalid => "invalid"
    else => "unavailable"
  }
}

pure bytes_text(value: Int?) -> Str {
  if value == null {
    return "unavailable"
  }
  let amount = value ?? 0
  if amount >= 1073741824 {
    return f"{(amount.float() / 1073741824.0).format(precision: 2)} GiB"
  }
  if amount >= 1048576 {
    return f"{(amount.float() / 1048576.0).format(precision: 2)} MiB"
  }
  if amount >= 1024 {
    return f"{(amount.float() / 1024.0).format(precision: 2)} KiB"
  }
  f"{amount} B"
}

pure field_text(value: Int?, state: report.ObservationState) -> Str {
  if value == null {
    return state_text(state)
  }
  bytes_text(value)
}

pure counter_text(value: rates.ByteValue) -> Str {
  if value.value == null {
    return match value.state {
      rates.Invalid => "invalid"
      rates.Reset => "reset"
      else => "unavailable"
    }
  }
  bytes_text(value.value)
}

pure uptime_text(value: syscpu.Uptime) -> Str {
  if value.seconds == null {
    return state_text(value.state)
  }
  f"{(value.seconds ?? 0.0).format(precision: 1)} s"
}

pure load_text(value: syscpu.LoadAverage) -> Str {
  if value.state != report.Observed or value.values.len() < 3 {
    return state_text(value.state)
  }
  f"{value.values[0].format(precision: 2)} {value.values[1].format(precision: 2)} {value.values[2].format(precision: 2)}"
}

pure process_text(row: model.ProcessRow, local_display: List[processes.LocalProcessDisplay]) -> Str {
  let account = if let display = ui.local_display(row, local_display) {
    display.user_name
  } else if row.uid == null {
    "?"
  } else {
    f"{row.uid ?? 0}"
  }
  let user_label = if row.uid_state == model.Stale { f"~{account}" } else { account }
  let command = if let display = ui.local_display(row, local_display) {
    display.command_line ?? row.command
  } else {
    row.command
  }
  f"{row.pid} {user_label} {rate_text(row.cpu_percent)} {bytes_text(row.rss_bytes)} {command}"
}

pure memory_content(value: model.Model) -> List[Str] {
  [
    f"Total {field_text(value.memory.total_bytes, value.memory.total_state)}",
    f"Available {field_text(value.memory.available_bytes, value.memory.available_state)}",
    f"Used {counter_text(value.memory.used_bytes)}",
    if value.memory.swap_total_state == report.Absent and value.memory.swap_free_state == report.Absent {
      "Swap absent"
    } else {
      f"Swap total {field_text(value.memory.swap_total_bytes, value.memory.swap_total_state)} used {counter_text(value.memory.swap_used_bytes)}"
    },
  ]
}

pure network_content(value: model.Model) -> List[Str] {
  if value.network.interface_name == null {
    return [f"Interface {state_text(value.network.source_state)}"]
  }
  [
    f"Interface {value.network.interface_name ?? ""}",
    f"RX {byte_rate_text(value.network.rx_bytes_per_second)} {bytes_text(value.network.rx_bytes)} total",
    f"TX {byte_rate_text(value.network.tx_bytes_per_second)} {bytes_text(value.network.tx_bytes)} total",
  ]
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
    ui.Pid | ui.ThreadCount | ui.Memory | ui.CpuDirect | ui.CpuLazy => if state.reverse { "ascending" } else { "descending" }
    ui.Name | ui.Cmd | ui.UserName => if state.reverse { "descending" } else { "ascending" }
  }
  let sort_line = f"Sort {sort_name} {direction} | r reverse"
  let filter_line = if state.editing_filter {
    f"Filter: {state.filter_draft}_ | Enter accept, Esc cancel"
  } else if state.filter != "" {
    f"Filter: {state.filter} | f edit, Del clear"
  } else {
    f"{sort_line} | f filter"
  }
  var lines = ["PID USER CPU% RSS COMMAND", filter_line]
  if value.process_state != report.Observed {
    lines += [f"Processes {state_text(value.process_state)}"]
  } else if value.processes.is_empty() {
    lines += ["No visible processes"]
  } else {
    let rows = ui.visible_processes(value, state, local_display)
    if rows.is_empty() {
      lines += ["No matching processes"]
    } else {
      for index in range(capacity) {
        let at = state.scroll + index
        if at < rows.len() {
          let row = rows[at]
          lines += [surface.selected_row(process_text(row, local_display), ui.is_selected(state, row), width)]
        } else {
          lines += [""]
        }
      }
    }
  }
  lines
}

## Returns the process rows that fit below the fixed panel headings.
export pure process_capacity(height: Int) -> Int {
  if height < 24 { return 0 }
  let remaining = height - 1
  let top_height = remaining / 2
  let bottom_height = remaining - top_height
  if bottom_height <= 5 { 0 } else { bottom_height - 5 }
}

## Renders a default process view at the injected window size.
export pure render(value: model.Model, width: Int, height: Int) -> Screen {
  render_view(value, ui.initial(value, process_capacity(height)), width, height, [])
}

## Renders current selection, sort, filter, and scroll state without I/O.
export pure render_view(
  value: model.Model,
  state: ui.State,
  width: Int,
  height: Int,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> Screen {
  if width < 80 or height < 24 {
    return {
      width: width,
      height: height,
      too_small: true,
      lines: [fit("btop: terminal too small (minimum 80x24)", width)],
    }
  }

  let header = fit(
    f"btop | CPU {rate_text(value.cpu_percent)} | uptime {uptime_text(value.uptime)} | load {load_text(value.load)} | q quit",
    width,
  )
  let left_width = width / 2
  let right_width = width - left_width
  let remaining_height = height - 1
  let top_height = remaining_height / 2
  let bottom_height = remaining_height - top_height
  let cpu_panel = panel(
    left_width,
    top_height,
    "CPU",
    [f"Usage {rate_text(value.cpu_percent)}", f"Uptime {uptime_text(value.uptime)}", f"Load {load_text(value.load)}"],
  )
  let memory_rows = panel(right_width, top_height, "Memory", memory_content(value))
  let network_rows = panel(left_width, bottom_height, "Network", network_content(value))
  let view = ui.resize(state, value, process_capacity(height))
  let process_rows = panel(
    right_width,
    bottom_height,
    "Processes",
    process_content(value, view, process_capacity(height), right_width - 2, local_display),
  )
  var lines = [header]
  for index in range(top_height) {
    lines += [f"{cpu_panel[index]}{memory_rows[index]}"]
  }
  for index in range(bottom_height) {
    lines += [f"{network_rows[index]}{process_rows[index]}"]
  }
  {width: width, height: height, too_small: false, lines: lines}
}
