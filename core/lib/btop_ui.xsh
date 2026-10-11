##! Reduces read-only process-list input into stable display state.
##!
##! Selection names a process lifetime, so sorting and new samples cannot
##! silently move the selection to a different PID reuse.
use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_rates as rates

const MAX_FILTER_CHARS = 128

## Names one process lifetime independently of its current sort position.
export type Identity = {pid: Int, start_ticks: Int}

## Selects one supported process-list ordering.
export enum Sort { Cpu, Pid, Memory }

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

pure identity(row: model.ProcessRow) -> Identity {
  {pid: row.pid, start_ticks: row.start_ticks}
}

pure same_identity(left: Identity?, right: Identity) -> Bool {
  left != null and (left ?? right) == right
}

pure matches(row: model.ProcessRow, filter: Str) -> Bool {
  if filter == "" { return true }
  let needle = filter.lower()
  f"{row.pid}".find(needle) != null or
    (if row.uid == null { "?" } else { f"{row.uid ?? 0}" }).lower().find(needle) != null or
    row.command.lower().find(needle) != null
}

pure rate_before(left: rates.Rate, right: rates.Rate, reverse: Bool) -> Bool {
  if left.state == rates.Observed and right.state != rates.Observed { return true }
  if left.state != rates.Observed and right.state == rates.Observed { return false }
  if left.state != rates.Observed { return false }
  if left.value == null and right.value != null { return false }
  if left.value != null and right.value == null { return true }
  let left_value = left.value ?? 0.0
  let right_value = right.value ?? 0.0
  if left_value == right_value { return false }
  if reverse { left_value < right_value } else { left_value > right_value }
}

pure optional_int_before(left: Int?, right: Int?, reverse: Bool) -> Bool {
  if left == null and right != null { return false }
  if left != null and right == null { return true }
  let left_value = left ?? 0
  let right_value = right ?? 0
  if left_value == right_value { return false }
  if reverse { left_value < right_value } else { left_value > right_value }
}

pure before(left: model.ProcessRow, right: model.ProcessRow, state: State) -> Bool {
  let primary_before = match state.sort {
    Cpu => rate_before(left.cpu_percent, right.cpu_percent, state.reverse)
    Pid => if state.reverse { left.pid > right.pid } else { left.pid < right.pid }
    Memory => optional_int_before(left.rss_bytes, right.rss_bytes, state.reverse)
  }
  if primary_before { return true }
  let right_before = match state.sort {
    Cpu => rate_before(right.cpu_percent, left.cpu_percent, state.reverse)
    Pid => if state.reverse { right.pid > left.pid } else { right.pid < left.pid }
    Memory => optional_int_before(right.rss_bytes, left.rss_bytes, state.reverse)
  }
  if right_before { return false }
  if left.pid != right.pid { return left.pid < right.pid }
  left.start_ticks < right.start_ticks
}

pure insert(rows: List[model.ProcessRow], row: model.ProcessRow, state: State) -> List[model.ProcessRow] {
  var result: List[model.ProcessRow] = []
  var inserted = false
  for current in rows {
    if ! inserted and before(row, current, state) {
      result += [row]
      inserted = true
    }
    result += [current]
  }
  if ! inserted { result += [row] }
  result
}

## Applies filtering and stable sorting to the complete process set.
export pure visible_processes(value: model.Model, state: State) -> List[model.ProcessRow] {
  var rows: List[model.ProcessRow] = []
  let filter = if state.editing_filter { state.filter_draft } else { state.filter }
  for row in value.processes {
    if matches(row, filter) { rows = insert(rows, row, state) }
  }
  rows
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

pure reconcile(state: State, value: model.Model) -> State {
  let rows = visible_processes(value, state)
  if rows.is_empty() { return {...state, selected: null, scroll: 0} }
  let selected = if selected_index(rows, state.selected) != null {
    state.selected
  } else {
    identity(rows[0])
  }
  {...state, selected: selected, scroll: bounded_scroll(state, rows, selected)}
}

## Keeps a view attached to live process lifetimes after each completed sample.
export pure refresh(state: State, value: model.Model) -> State {
  reconcile(state, value)
}

## Identifies the selected process row without relying on its list position.
export pure is_selected(state: State, row: model.ProcessRow) -> Bool {
  same_identity(state.selected, identity(row))
}

## Starts a CPU-sorted view and selects the first process lifetime.
export pure initial(value: model.Model, visible_rows: Int) -> State {
  reconcile({
    selected: null,
    sort: .Cpu,
    reverse: false,
    filter: "",
    editing_filter: false,
    filter_draft: "",
    scroll: 0,
    visible_rows: if visible_rows < 0 { 0 } else { visible_rows },
  }, value)
}

## Keeps the selected identity visible after terminal geometry changes.
export pure resize(state: State, value: model.Model, visible_rows: Int) -> State {
  reconcile({...state, visible_rows: if visible_rows < 0 { 0 } else { visible_rows}}, value)
}

pure next_sort(sort: Sort) -> Sort {
  match sort { Cpu => .Pid, Pid => .Memory, Memory => .Cpu }
}

pure previous_sort(sort: Sort) -> Sort {
  match sort { Cpu => .Memory, Memory => .Pid, Pid => .Cpu }
}

pure drop_last(text: Str) -> Str {
  let characters = text.split("")
  if characters.is_empty() { return "" }
  characters |> take(characters.len() - 1) |> join("")
}

pure move(state: State, value: model.Model, amount: Int) -> State {
  let rows = visible_processes(value, state)
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

pure browse_key(state: State, value: model.Model, key: input.Key) -> State {
  match key {
    input.Up => move(state, value, -1)
    input.Down => move(state, value, 1)
    input.PageUp => move(state, value, -(if state.visible_rows < 1 { 1 } else { state.visible_rows }))
    input.PageDown => move(state, value, if state.visible_rows < 1 { 1 } else { state.visible_rows })
    input.Home => {
      let rows = visible_processes(value, state)
      if rows.is_empty() { {...state, selected: null, scroll: 0} } else { {...state, selected: identity(rows[0]), scroll: 0} }
    }
    input.End => {
      let rows = visible_processes(value, state)
      if rows.is_empty() { {...state, selected: null, scroll: 0} } else { move(state, value, rows.len()) }
    }
    input.Left => reconcile({...state, sort: previous_sort(state.sort)}, value)
    input.Right => reconcile({...state, sort: next_sort(state.sort)}, value)
    input.Text("r") => reconcile({...state, reverse: !state.reverse}, value)
    input.Text("f") | input.Text("/") => {...state, editing_filter: true, filter_draft: state.filter}
    input.Delete => reconcile({...state, filter: "", filter_draft: ""}, value)
    else => state
  }
}

## Applies one decoded key while treating paste as inert text.
export pure reduce(state: State, event: input.Event, value: model.Model) -> State {
  match event {
    input.Pressed(key) => {
      let updated = if state.editing_filter { edit_key(state, key) } else { browse_key(state, value, key) }
      reconcile(updated, value)
    }
    else => state
  }
}

## Replays a complete read without allowing paste payload keys to activate.
export pure reduce_events(state: State, events: List[input.Event], value: model.Model) -> State {
  var current = state
  for event in events { current = reduce(current, event, value) }
  current
}

## Returns the selected process position in the filtered sorted rows.
export pure selection_position(state: State, value: model.Model) -> Int? {
  selected_index(visible_processes(value, state), state.selected)
}
