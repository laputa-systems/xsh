##! Reduces read-only process-list input into stable display state.
##!
##! Selection names a process lifetime, so sorting and new samples cannot
##! silently move the selection to a different PID reuse.
use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.lib.sys_proc as processes

const MAX_FILTER_CHARS = 128

## Names one process lifetime independently of its current sort position.
export type Identity = {pid: Int, start_ticks: Int}

## Selects one pinned process-list ordering.
export enum Sort { Pid, Name, Cmd, ThreadCount, UserName, Memory, CpuDirect, CpuLazy }

## Retains process identity, list view, and an optional filter edit draft.
export type State = {
  selected: Identity?,
  sort: Sort,
  reverse: Bool,
  filter: Str,
  editing_filter: Bool,
  filter_draft: Str,
  scroll: Int,
  visible_rows: Int,
}
type LocalDisplayIndex = Map[Int, processes.LocalProcessDisplay]

pure identity(row: model.ProcessRow) -> Identity {
  {pid: row.pid, start_ticks: row.start_ticks}
}

pure same_identity(left: Identity?, right: Identity) -> Bool {
  left != null and (left ?? right) == right
}

## Finds display-only metadata for the same process lifetime.
export pure local_display(row: model.ProcessRow, values: List[processes.LocalProcessDisplay]) -> processes.LocalProcessDisplay? {
  for value in values {
    if value.pid == row.pid and value.start_ticks == row.start_ticks { return value }
  }
  null
}

pure indexed_local_display(row: model.ProcessRow, values: LocalDisplayIndex) -> processes.LocalProcessDisplay? {
  if let Ok(value) = values.get(row.pid) {
    if value.start_ticks == row.start_ticks { return value }
  }
  null
}

pure command_text(row: model.ProcessRow, values: LocalDisplayIndex) -> Str {
  if let value = indexed_local_display(row, values) {
    if let command = value.command_line { return command }
  }
  row.command
}

pure user_text(row: model.ProcessRow, values: LocalDisplayIndex) -> Str {
  if let value = indexed_local_display(row, values) { return value.user_name }
  if row.uid == null { "?" } else { f"{row.uid ?? 0}" }
}

pure matches(row: model.ProcessRow, filter: Str, values: LocalDisplayIndex) -> Bool {
  if filter == "" { return true }
  let needle = filter.lower()
  f"{row.pid}".find(needle) != null or
    (if row.uid == null { "?" } else { f"{row.uid ?? 0}" }).lower().find(needle) != null or
    row.command.lower().find(needle) != null or
    command_text(row, values).lower().find(needle) != null or
    user_text(row, values).lower().find(needle) != null
}

pure rate_available(value: rates.Rate) -> Bool {
  value.state == rates.Observed and value.value != null
}

pure cpu_sort_value(value: rates.Rate, scale: Int) -> Int {
  if !rate_available(value) { return 0 }
  let scaled = (value.value ?? 0.0) * scale.float()
  if let Ok(units) = scaled.round() {
    units
  } else if scaled < 0.0 {
    -9223372036854775807
  } else {
    9223372036854775807
  }
}

pure cpu_value(value: rates.Rate) -> Float {
  if rate_available(value) { value.value ?? 0.0 } else { 0.0 }
}

pure promote_cpu_spikes(rows: List[model.ProcessRow]) -> List[model.ProcessRow] {
  if rows.is_empty() { return rows }
  var maximum = 10.0
  var target = 30.0
  var offset = 0
  var moved = 0
  var ordered = rows
  for index in range(ordered.len()) {
    let row = ordered[index]
    let value = cpu_value(row.cpu_percent)
    if index <= 5 and value > maximum {
      maximum = value
    } else if index == 6 {
      target = if maximum > 30.0 { maximum } else { 10.0 }
    }
    if index == offset and value > 30.0 {
      offset += 1
    } else {
      if value > target {
        ordered = ordered[..offset]
          .extend([row])
          .extend(ordered[offset..index])
          .extend(ordered[index + 1..])
        moved += 1
        if moved > 10 { break }
      }
    }
  }
  ordered
}

pure sort_processes(
  rows: List[model.ProcessRow],
  state: State,
  values: LocalDisplayIndex,
) -> List[model.ProcessRow] {
  let by_pid = rows |> sort-by { |row| row.pid }
  let ordered = match state.sort {
    Pid => by_pid |> sort-by(desc: !state.reverse) { |row| row.pid }
    Name => by_pid |> sort-by(desc: state.reverse) { |row| row.command }
    Cmd => by_pid |> sort-by(desc: state.reverse) { |row| command_text(row, values) }
    ThreadCount => by_pid
      |> sort-by(desc: !state.reverse) { |row| row.thread_count ?? 0 }
      |> sort-by { |row| if row.thread_count == null { 1 } else { 0 } }
    UserName => by_pid |> sort-by(desc: state.reverse) { |row| user_text(row, values) }
    Memory => by_pid
      |> sort-by(desc: !state.reverse) { |row| row.rss_bytes ?? 0 }
      |> sort-by { |row| if row.rss_bytes == null { 1 } else { 0 } }
    CpuDirect => by_pid
      |> sort-by(desc: !state.reverse) { |row| cpu_sort_value(row.cpu_percent, 10) }
      |> sort-by { |row| if rate_available(row.cpu_percent) { 0 } else { 1 } }
    CpuLazy => by_pid
      |> sort-by(desc: !state.reverse) { |row| cpu_sort_value(row.cpu_average_percent, 1000000) }
      |> sort-by { |row| if rate_available(row.cpu_average_percent) { 0 } else { 1 } }
  }
  if state.sort == CpuLazy and !state.reverse { promote_cpu_spikes(ordered) } else { ordered }
}

## Applies filtering and stable sorting to the complete process set.
export pure visible_processes(
  value: model.Model,
  state: State,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> List[model.ProcessRow] {
  var display_index: LocalDisplayIndex = {}
  for display in local_display {
    display_index = display_index.set(display.pid, display)
  }
  var rows: List[model.ProcessRow] = []
  let filter = if state.editing_filter { state.filter_draft } else { state.filter }
  for row in value.processes {
    if matches(row, filter, display_index) { rows += [row] }
  }
  sort_processes(rows, state, display_index)
}

pure selected_index(rows: List[model.ProcessRow], selected: Identity?) -> Int? {
  if selected == null { return null }
  for index in range(rows.len()) {
    if same_identity(selected, identity(rows[index])) { return index }
  }
  null
}

pure bounded_scroll(state: State, rows: List[model.ProcessRow], selected: Identity?) -> Int {
  if rows.is_empty() { return 0 }
  let visible = if state.visible_rows < 1 { 1 } else { state.visible_rows }
  let maximum = if rows.len() > visible { rows.len() - visible } else { 0 }
  let current = if state.scroll < 0 { 0 } else if state.scroll > maximum { maximum } else { state.scroll }
  if let index = selected_index(rows, selected) {
    if index < current { return index }
    if index >= current + visible { return index - visible + 1 }
  }
  current
}

pure reconcile(
  state: State,
  value: model.Model,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> State {
  let rows = visible_processes(value, state, local_display)
  if rows.is_empty() { return {...state, selected: null, scroll: 0} }
  let selected = if selected_index(rows, state.selected) != null {
    state.selected
  } else {
    identity(rows[0])
  }
  {...state, selected: selected, scroll: bounded_scroll(state, rows, selected)}
}

## Keeps a view attached to live process lifetimes after each completed sample.
export pure refresh(
  state: State,
  value: model.Model,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> State {
  reconcile(state, value, local_display)
}

## Identifies the selected process row without relying on its list position.
export pure is_selected(state: State, row: model.ProcessRow) -> Bool {
  same_identity(state.selected, identity(row))
}

## Starts a CPU-sorted view and selects the first process lifetime.
export pure initial(value: model.Model, visible_rows: Int) -> State {
  reconcile({
    selected: null,
    sort: .CpuLazy,
    reverse: false,
    filter: "",
    editing_filter: false,
    filter_draft: "",
    scroll: 0,
    visible_rows: if visible_rows < 0 { 0 } else { visible_rows },
  }, value)
}

## Keeps the selected identity visible after terminal geometry changes.
export pure resize(
  state: State,
  value: model.Model,
  visible_rows: Int,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> State {
  reconcile({...state, visible_rows: if visible_rows < 0 { 0 } else { visible_rows}}, value, local_display)
}

pure next_sort(sort: Sort) -> Sort {
  match sort {
    Pid => .Name
    Name => .Cmd
    Cmd => .ThreadCount
    ThreadCount => .UserName
    UserName => .Memory
    Memory => .CpuDirect
    CpuDirect => .CpuLazy
    CpuLazy => .Pid
  }
}

pure previous_sort(sort: Sort) -> Sort {
  match sort {
    Pid => .CpuLazy
    Name => .Pid
    Cmd => .Name
    ThreadCount => .Cmd
    UserName => .ThreadCount
    Memory => .UserName
    CpuDirect => .Memory
    CpuLazy => .CpuDirect
  }
}

pure drop_last(text: Str) -> Str {
  let characters = text.split("")
  if characters.is_empty() { return "" }
  characters |> take(characters.len() - 1) |> join("")
}

pure move(
  state: State,
  value: model.Model,
  amount: Int,
  local_display: List[processes.LocalProcessDisplay],
) -> State {
  let rows = visible_processes(value, state, local_display)
  if rows.is_empty() { return {...state, selected: null, scroll: 0} }
  let start = selected_index(rows, state.selected) ?? 0
  let target = if start + amount < 0 {
    0
  } else if start + amount >= rows.len() {
    rows.len() - 1
  } else {
    start + amount
  }
  let selected = identity(rows[target])
  {...state, selected: selected, scroll: bounded_scroll(state, rows, selected)}
}

pure edit_key(state: State, key: input.Key) -> State {
  match key {
    input.Text(text) => if state.filter_draft.count_chars() < MAX_FILTER_CHARS {
      {...state, filter_draft: state.filter_draft + text}
    } else { state }
    input.Backspace => {...state, filter_draft: drop_last(state.filter_draft)}
    input.Delete => {...state, filter_draft: ""}
    input.Enter => {...state, filter: state.filter_draft, editing_filter: false}
    input.Escape => {...state, editing_filter: false, filter_draft: ""}
    else => state
  }
}

pure browse_key(
  state: State,
  value: model.Model,
  key: input.Key,
  local_display: List[processes.LocalProcessDisplay],
) -> State {
  match key {
    input.Up => move(state, value, -1, local_display)
    input.Down => move(state, value, 1, local_display)
    input.PageUp => move(state, value, -(if state.visible_rows < 1 { 1 } else { state.visible_rows }), local_display)
    input.PageDown => move(state, value, if state.visible_rows < 1 { 1 } else { state.visible_rows }, local_display)
    input.Home => {
      let rows = visible_processes(value, state, local_display)
      if rows.is_empty() { {...state, selected: null, scroll: 0} } else { {...state, selected: identity(rows[0]), scroll: 0} }
    }
    input.End => {
      let rows = visible_processes(value, state, local_display)
      if rows.is_empty() { {...state, selected: null, scroll: 0} } else { move(state, value, rows.len(), local_display) }
    }
    input.Left => reconcile({...state, sort: previous_sort(state.sort)}, value, local_display)
    input.Right => reconcile({...state, sort: next_sort(state.sort)}, value, local_display)
    input.Text("r") => reconcile({...state, reverse: !state.reverse}, value, local_display)
    input.Text("f") | input.Text("/") => {...state, editing_filter: true, filter_draft: state.filter}
    input.Delete => reconcile({...state, filter: "", filter_draft: ""}, value)
    else => state
  }
}

## Applies one decoded key while treating paste as inert text.
export pure reduce(
  state: State,
  event: input.Event,
  value: model.Model,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> State {
  match event {
    input.Pressed(key) => {
      let updated = if state.editing_filter { edit_key(state, key) } else { browse_key(state, value, key, local_display) }
      reconcile(updated, value, local_display)
    }
    else => state
  }
}

## Replays a complete read without allowing paste payload keys to activate.
export pure reduce_events(
  state: State,
  events: List[input.Event],
  value: model.Model,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> State {
  var current = state
  for event in events { current = reduce(current, event, value, local_display) }
  current
}

## Returns the selected process position in the filtered sorted rows.
export pure selection_position(
  state: State,
  value: model.Model,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> Int? {
  selected_index(visible_processes(value, state, local_display), state.selected)
}
