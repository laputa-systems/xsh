##! Reduces read-only process-list input into stable display state.
##!
##! Selection names a process lifetime, so sorting and new samples cannot
##! silently move the selection to a different PID reuse.
use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.lib.btop_process_tree as process_tree
use core.lib.btop_defaults as defaults
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
  tree_view: Bool,
  collapsed: Set[Str],
  aggregate_children: Bool,
  process_memory_bytes: Bool,
  process_cpu_per_core: Bool,
  network_auto: Bool,
  network_sync: Bool,
  network_interface: Str?,
  network_offset_interface: Str?,
  network_offset_ifindex: Int?,
  network_rx_offset: Int?,
  network_tx_offset: Int?,
  show_disks: Bool,
  disk_io_mode: Bool,
}
type LocalDisplayIndex = Map[Int, processes.LocalProcessDisplay]

## Holds the row and tree metadata needed by the process panel.
export type VisibleRow = {
  row: model.ProcessRow,
  depth: Int,
  has_children: Bool,
  collapsed: Bool,
  cpu_total: rates.Rate,
  cpu_total_per_core: rates.Rate,
  rss_total_bytes: Int?,
}

pure identity(row: model.ProcessRow) -> Identity {
  {pid: row.pid, start_ticks: row.start_ticks}
}

pure identity_key(identity: Identity) -> Str {
  f"{identity.pid}:{identity.start_ticks}"
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

pure displayed_cpu(row: model.ProcessRow, state: State) -> rates.Rate {
  if state.process_cpu_per_core { row.cpu_percent_per_core } else { row.cpu_percent }
}

pure promote_cpu_spikes(rows: List[model.ProcessRow], state: State) -> List[model.ProcessRow] {
  if rows.is_empty() { return rows }
  var maximum = 10.0
  var target = 30.0
  var offset = 0
  var moved = 0
  var ordered = rows
  for index in range(ordered.len()) {
    let row = ordered[index]
    let value = cpu_value(displayed_cpu(row, state))
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
      |> sort-by(desc: !state.reverse) { |row| cpu_sort_value(displayed_cpu(row, state), 10) }
      |> sort-by { |row| if rate_available(displayed_cpu(row, state)) { 0 } else { 1 } }
    CpuLazy => by_pid
      |> sort-by(desc: !state.reverse) { |row| cpu_sort_value(row.cpu_average_percent, 1000000) }
      |> sort-by { |row| if rate_available(row.cpu_average_percent) { 0 } else { 1 } }
  }
  if state.sort == CpuLazy and !state.reverse { promote_cpu_spikes(ordered, state) } else { ordered }
}

pure local_display_index(values: List[processes.LocalProcessDisplay]) -> LocalDisplayIndex {
  var display_index: LocalDisplayIndex = {}
  for display in values {
    display_index = display_index.set(display.pid, display)
  }
  display_index
}

pure filter_text(state: State) -> Str {
  if state.editing_filter { state.filter_draft } else { state.filter }
}

pure filter_matches(
  rows: List[model.ProcessRow],
  state: State,
  values: LocalDisplayIndex,
) -> List[model.ProcessRow] {
  var visible: List[model.ProcessRow] = []
  let filter = filter_text(state)
  for row in rows {
    if matches(row, filter, values) { visible += [row] }
  }
  visible
}

pure process_tree_rows(
  value: model.Model,
  state: State,
  values: LocalDisplayIndex,
) -> List[VisibleRow] {
  let ordered = sort_processes(value.processes, state, values)
  let filtered = filter_matches(ordered, state, values)
  if ! state.tree_view {
    return [
      {
        row: row,
        depth: 0,
        has_children: false,
        collapsed: false,
        cpu_total: row.cpu_percent,
        cpu_total_per_core: row.cpu_percent_per_core,
        rss_total_bytes: row.rss_bytes,
      }
      for row in filtered
    ]
  }

  let hierarchy = process_tree.build(value.processes, state.aggregate_children, ordered)
  var matching: Set[Int] = set.empty()
  for row in filtered { matching = matching.add(row.pid) }
  var included: Set[Int] = set.empty()
  if filter_text(state) == "" {
    for pid in hierarchy.preorder { included = included.add(pid) }
  } else {
    for pid in hierarchy.preorder {
      var cursor: Int? = pid
      for _ in range(hierarchy.preorder.len()) {
        if cursor == null { break }
        let current = cursor ?? pid
        if current in matching {
          included = included.add(pid)
          break
        }
        if let Ok(node) = hierarchy.nodes.get(current) {
          cursor = node.parent_pid
        } else {
          cursor = null
        }
      }
      if pid in matching {
        if let Ok(node) = hierarchy.nodes.get(pid) {
          var parent = node.parent_pid
          for _ in range(hierarchy.preorder.len()) {
            if parent == null { break }
            let parent_pid = parent ?? pid
            included = included.add(parent_pid)
            if let Ok(parent_node) = hierarchy.nodes.get(parent_pid) {
              parent = parent_node.parent_pid
            } else {
              parent = null
            }
          }
        }
      }
    }
  }

  var visible: List[VisibleRow] = []
  var hidden_depth: Int? = null
  for pid in hierarchy.preorder {
    let node = hierarchy.nodes[pid]
    if hidden_depth != null {
      if node.depth > (hidden_depth ?? node.depth) { continue }
      hidden_depth = null
    }
    if pid not in included { continue }
    let key = identity_key(identity(node.row))
    let has_children = ! node.children.is_empty()
    let collapsed = has_children and key in state.collapsed
    visible += [{
      row: node.row,
      depth: node.depth,
      has_children: has_children,
      collapsed: collapsed,
      cpu_total: node.cpu_total,
      cpu_total_per_core: node.cpu_total_per_core,
      rss_total_bytes: node.rss_total_bytes,
    }]
    if collapsed { hidden_depth = node.depth }
  }
  visible
}

## Returns filtered, sorted rows with deterministic tree and aggregate state.
export pure visible_tree_rows(
  value: model.Model,
  state: State,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> List[VisibleRow] {
  process_tree_rows(value, state, local_display_index(local_display))
}

## Applies filtering and stable sorting to the complete process set.
export pure visible_processes(
  value: model.Model,
  state: State,
  local_display: List[processes.LocalProcessDisplay] = [],
) -> List[model.ProcessRow] {
  [entry.row for entry in visible_tree_rows(value, state, local_display)]
}

pure active_collapses(state: State, value: model.Model) -> Set[Str] {
  var identities: Set[Str] = set.empty()
  for row in value.processes { identities = identities.add(identity_key(identity(row))) }
  var retained: Set[Str] = set.empty()
  for key in state.collapsed {
    if key in identities { retained = retained.add(key) }
  }
  retained
}

pure active_network_interface(state: State, value: model.Model) -> Str? {
  if state.network_interface != null {
    for interface in value.network.interfaces {
      if interface.name == (state.network_interface ?? "") { return state.network_interface }
    }
  }
  value.network.interface_name
}

## Returns the selected interface's already-derived rates and counters.
export pure selected_network_interface(state: State, value: model.Model) -> model.NetworkMetric? {
  model.network_metric(value.network, active_network_interface(state, value))
}

## Returns a selected network counter after applying the optional display offset.
export pure network_total_bytes(state: State, interface: model.NetworkMetric?, receive: Bool) -> Int? {
  if let selected = interface {
    let current = if receive { selected.rx_bytes } else { selected.tx_bytes }
    if state.network_offset_interface != selected.name or state.network_offset_ifindex != selected.ifindex { return current }
    let offset = if receive { state.network_rx_offset } else { state.network_tx_offset }
    if offset == null { return current }
    if current < (offset ?? 0) { return null }
    return current - (offset ?? 0)
  }
  null
}

pure toggle_network_offset(state: State, value: model.Model) -> State {
  if let interface = selected_network_interface(state, value) {
    if state.network_offset_interface == interface.name and state.network_offset_ifindex == interface.ifindex and
        state.network_rx_offset != null and state.network_tx_offset != null {
      return {
        ...state,
        network_offset_interface: null,
        network_offset_ifindex: null,
        network_rx_offset: null,
        network_tx_offset: null,
      }
    }
    return {
      ...state,
      network_offset_interface: interface.name,
      network_offset_ifindex: interface.ifindex,
      network_rx_offset: interface.rx_bytes,
      network_tx_offset: interface.tx_bytes,
    }
  }
  state
}

pure select_network_interface(state: State, value: model.Model, direction: Int) -> State {
  let interfaces = value.network.interfaces
  if interfaces.is_empty() { return state }
  let selected_name = active_network_interface(state, value)
  var selected_index: Int? = null
  for index in range(interfaces.len()) {
    if interfaces[index].name == selected_name { selected_index = index }
  }
  let current = selected_index ?? 0
  let next = if direction < 0 {
    if current == 0 { interfaces.len() - 1 } else { current - 1 }
  } else if current + 1 == interfaces.len() {
    0
  } else {
    current + 1
  }
  {
    ...state,
    network_interface: interfaces[next].name,
    network_offset_interface: null,
    network_offset_ifindex: null,
    network_rx_offset: null,
    network_tx_offset: null,
  }
}

pure collapse_selected(state: State, value: model.Model, action: Str) -> State {
  if state.selected == null { return state }
  let tree = process_tree.build(value.processes)
  let selected = state.selected ?? {pid: 0, start_ticks: 0}
  if let Ok(node) = tree.nodes.get(selected.pid) {
    if node.row.start_ticks != selected.start_ticks or node.children.is_empty() { return state }
    let key = identity_key(selected)
    var collapsed = state.collapsed
    if action == "collapse" {
      collapsed = collapsed.add(key)
    } else if action == "expand" {
      collapsed = collapsed.remove(key)
    } else if key in collapsed {
      collapsed = collapsed.remove(key)
    } else {
      collapsed = collapsed.add(key)
    }
    return {...state, collapsed: collapsed}
  }
  state
}

pure toggle_selected_children(state: State, value: model.Model) -> State {
  if state.selected == null { return state }
  let tree = process_tree.build(value.processes)
  let selected = state.selected ?? {pid: 0, start_ticks: 0}
  if let Ok(node) = tree.nodes.get(selected.pid) {
    if node.row.start_ticks != selected.start_ticks { return state }
    var collapsed = state.collapsed
    for child_pid in node.children {
      if let Ok(child) = tree.nodes.get(child_pid) {
        if ! child.children.is_empty() {
          let key = identity_key(identity(child.row))
          collapsed = if key in collapsed { collapsed.remove(key) } else { collapsed.add(key) }
        }
      }
    }
    return {...state, collapsed: collapsed}
  }
  state
}

pure toggle_all_children(state: State, value: model.Model) -> State {
  let tree = process_tree.build(value.processes)
  var collapse = false
  for pid in tree.preorder {
    let node = tree.nodes[pid]
    if node.parent_pid != null and ! node.children.is_empty() and identity_key(identity(node.row)) not in state.collapsed {
      collapse = true
    }
  }
  var collapsed = state.collapsed
  for pid in tree.preorder {
    let node = tree.nodes[pid]
    if node.parent_pid != null and ! node.children.is_empty() {
      let key = identity_key(identity(node.row))
      collapsed = if collapse { collapsed.add(key) } else { collapsed.remove(key) }
    }
  }
  {...state, collapsed: collapsed}
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
  let current = {
    ...state,
    collapsed: active_collapses(state, value),
    network_interface: active_network_interface(state, value),
  }
  let rows = visible_processes(value, current, local_display)
  if rows.is_empty() { return {...current, selected: null, scroll: 0} }
  let selected = if selected_index(rows, current.selected) != null {
    current.selected
  } else {
    identity(rows[0])
  }
  {...current, selected: selected, scroll: bounded_scroll(current, rows, selected)}
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
    sort: if defaults.process_sorting() == "cpu lazy" { .CpuLazy } else { .Pid },
    reverse: defaults.process_reversed(),
    filter: "",
    editing_filter: false,
    filter_draft: "",
    scroll: 0,
    visible_rows: if visible_rows < 0 { 0 } else { visible_rows },
    tree_view: defaults.process_tree_enabled(),
    collapsed: set.empty(),
    aggregate_children: defaults.aggregate_process_children(),
    process_memory_bytes: defaults.process_memory_in_bytes(),
    process_cpu_per_core: defaults.process_per_core_enabled(),
    network_auto: defaults.network_auto_enabled(),
    network_sync: defaults.network_scales_synchronized(),
    network_interface: value.network.interface_name,
    network_offset_interface: null,
    network_offset_ifindex: null,
    network_rx_offset: null,
    network_tx_offset: null,
    show_disks: defaults.disks_enabled(),
    disk_io_mode: defaults.disk_io_mode_enabled(),
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
    input.Text("b") => reconcile(select_network_interface(state, value, -1), value, local_display)
    input.Text("n") => reconcile(select_network_interface(state, value, 1), value, local_display)
    input.Text("r") => reconcile({...state, reverse: !state.reverse}, value, local_display)
    input.Text("c") => reconcile({...state, process_cpu_per_core: !state.process_cpu_per_core}, value, local_display)
    input.Text("z") => reconcile(toggle_network_offset(state, value), value, local_display)
    input.Text("a") => reconcile({...state, network_auto: !state.network_auto}, value, local_display)
    input.Text("y") => reconcile({...state, network_sync: !state.network_sync}, value, local_display)
    input.Text("%") => reconcile({...state, process_memory_bytes: !state.process_memory_bytes}, value, local_display)
    input.Text("d") => {...state, show_disks: !state.show_disks}
    input.Text("i") => {...state, disk_io_mode: !state.disk_io_mode}
    input.Text("e") => reconcile({...state, tree_view: !state.tree_view}, value, local_display)
    input.Text("E") => if state.tree_view { reconcile(toggle_all_children(state, value), value, local_display) } else { state }
    input.Text("+") => if state.tree_view { reconcile(collapse_selected(state, value, "expand"), value, local_display) } else { state }
    input.Text("-") => if state.tree_view { reconcile(collapse_selected(state, value, "collapse"), value, local_display) } else { state }
    input.Text(" ") => if state.tree_view { reconcile(collapse_selected(state, value, "toggle"), value, local_display) } else { state }
    input.Text("C") => if state.tree_view { reconcile(toggle_selected_children(state, value), value, local_display) } else { state }
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
