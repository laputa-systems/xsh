use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.lib.btop_source as source
use core.lib.btop_ui as ui
use core.lib.sys_proc as processes
use core.tests.support.btop as fixture

proc capture(root: FsRoot, prefix: Path, at_ns: Int) [fs, error] -> Result[model.Observation, Error] {
  let frame_root = root.open_root(prefix)?
  defer frame_root.close()
  source.capture(frame_root, "ui-fixture", at_ns, 4096, 100)
}

proc prepare_frames(root: FsRoot) [fs, error] -> Result[List[model.Model], Error] {
  fixture.write_b3_frame(
    root,
    "frames/0",
    0,
    [
      {pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 10, uid: 1000},
      {pid: 43, start_ticks: 2000, cpu_ticks: 20, rss_pages: 20, uid: 1000},
      {pid: 44, start_ticks: 3000, cpu_ticks: 30, rss_pages: 30, uid: 1000},
    ],
  )
  fixture.write_b3_frame(
    root,
    "frames/1",
    1,
    [
      {pid: 42, start_ticks: 1000, cpu_ticks: 11, rss_pages: 10, uid: 1000},
      {pid: 43, start_ticks: 2000, cpu_ticks: 40, rss_pages: 20, uid: 1000},
      {pid: 44, start_ticks: 3000, cpu_ticks: 35, rss_pages: 30, uid: 1000},
    ],
  )
  fixture.write_b3_frame(
    root,
    "frames/2",
    2,
    [
      {pid: 42, start_ticks: 1000, cpu_ticks: 12, rss_pages: 10, uid: 1000},
      {pid: 43, start_ticks: 4000, cpu_ticks: 45, rss_pages: 21, uid: 1000},
      {pid: 44, start_ticks: 3000, cpu_ticks: 40, rss_pages: 30, uid: 1000},
    ],
  )
  root.write(
    p"frames/2/proc/stat",
    "cpu 220 0 100 1080 0 0 0 0 0 0\ncpu0 110 0 50 540 0 0 0 0 0 0\ncpu1 110 0 50 540 0 0 0 0 0 0\nbtime 1700000000\nprocesses 3\nprocs_running 1\nprocs_blocked 0\n",
  )
  let first = capture(root, p"frames/0", 1000000000)?
  let second = capture(root, p"frames/1", 2000000000)?
  let third = capture(root, p"frames/2", 3000000000)?
  Ok([model.step(first, null), model.step(second, first), model.step(third, second)])
}

pure press(state: ui.State, value: model.Model, key: input.Key) -> ui.State {
  ui.reduce(state, .Pressed(key), value)
}

test test_btop_ui_keeps_selection_by_process_lifetime_across_sort_and_pid_reuse {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let first = frames[1]
  let second = frames[2]
  let initial = ui.initial(first, 1)
  assert initial.sort == .CpuLazy
  assert [row.pid for row in ui.visible_processes(first, initial)] == [43, 42, 44]
  let direct = {...initial, sort: ui.CpuDirect}

  assert [row.pid for row in ui.visible_processes(first, direct)] == [43, 44, 42]
  assert direct.selected == {pid: 43, start_ticks: 2000}
  let after_pid_reuse = ui.refresh(direct, second)
  assert after_pid_reuse.selected == {pid: 44, start_ticks: 3000}
  assert after_pid_reuse.selected != {pid: 43, start_ticks: 2000}
  let selected = press(direct, first, input.Down)
  assert selected.selected == {pid: 44, start_ticks: 3000}
  assert selected.scroll == 1

  let reversed = press(selected, first, .Text("r"))
  assert reversed.reverse
  assert [row.pid for row in ui.visible_processes(first, reversed)] == [42, 44, 43]
  assert reversed.selected == selected.selected

  let by_pid = press(press(reversed, first, input.Right), first, input.Right)
  assert by_pid.sort == .Pid
  assert [row.pid for row in ui.visible_processes(first, by_pid)] == [42, 43, 44]
  assert by_pid.selected == selected.selected

  let replaced_pid = ui.resize(by_pid, second, 2)
  assert replaced_pid.selected == {pid: 44, start_ticks: 3000}
  assert replaced_pid.visible_rows == 2
}

test test_btop_ui_filter_edit_accept_cancel_clear_and_paste_are_explicit {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let value = frames[1]
  let initial = ui.initial(value, 2)

  let editing = press(initial, value, .Text("f"))
  assert editing.editing_filter
  let typed = ui.reduce_events(editing, [
    .Pressed(.Text("4")),
    .Pressed(.Text("2")),
    .Pressed(.Text("q")),
    .Pressed(input.Backspace),
  ], value)
  assert typed.filter_draft == "42"
  assert typed.filter == ""
  let accepted = press(typed, value, input.Enter)
  assert ! accepted.editing_filter
  assert accepted.filter == "42"
  assert [row.pid for row in ui.visible_processes(value, accepted)] == [42]

  let edit_again = press(accepted, value, .Text("/"))
  let canceled = press(press(edit_again, value, .Text("x")), value, input.Escape)
  assert ! canceled.editing_filter
  assert canceled.filter == "42"
  assert canceled.filter_draft == ""

  let clear = press(canceled, value, input.Delete)
  assert clear.filter == ""
  assert ui.visible_processes(value, clear).len() == 3

  let editing_again = press(clear, value, .Text("f"))
  let pasted = ui.reduce(editing_again, .Paste("q\r\n", false), value)
  assert pasted == editing_again
}

test test_btop_ui_resize_preserves_identity_and_clamps_scroll {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let value = frames[1]
  let initial = ui.initial(value, 1)
  let selected = press(press(initial, value, input.Down), value, input.Down)
  assert selected.selected == {pid: 44, start_ticks: 3000}
  assert selected.scroll == 2

  let larger = ui.resize(selected, value, 3)
  assert larger.selected == selected.selected
  assert larger.scroll == 0
  assert larger.visible_rows == 3

  let filtered = press(press(larger, value, .Text("f")), value, .Text("missing"))
  let accepted = press(filtered, value, input.Enter)
  assert accepted.selected == null
  assert accepted.scroll == 0
  assert ui.selection_position(accepted, value) == null
}

test test_btop_ui_process_tree_toggle_collapse_children_and_aggregate {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let base = frames[1]
  let rows = base.processes |> sort-by { |row| row.pid }
  let value: model.Model = model.Model(
    source_id: base.source_id,
    at_ns: base.at_ns,
    cpu_percent: base.cpu_percent,
    cpu_cores: base.cpu_cores,
    telemetry: base.telemetry,
    uptime: base.uptime,
    load: base.load,
    cpu_state: base.cpu_state,
    memory: base.memory,
    network: base.network,
    process_state: base.process_state,
    processes: [
      {...rows[0], parent_pid: 0, cpu_percent: {state: rates.Observed, value: 10.0}, cpu_percent_per_core: {state: rates.Observed, value: 20.0}, cpu_average_percent: {state: rates.Observed, value: 10.0}, rss_bytes: 100},
      {...rows[1], parent_pid: 42, cpu_percent: {state: rates.Observed, value: 3.0}, cpu_percent_per_core: {state: rates.Observed, value: 6.0}, cpu_average_percent: {state: rates.Observed, value: 3.0}, rss_bytes: 20},
      {...rows[2], parent_pid: 43, cpu_percent: {state: rates.Observed, value: 2.0}, cpu_percent_per_core: {state: rates.Observed, value: 4.0}, cpu_average_percent: {state: rates.Observed, value: 2.0}, rss_bytes: 10},
    ],
    storage: base.storage,
  )
  let initial = ui.initial(value, 3)
  assert ! initial.tree_view
  assert initial.sort == .CpuLazy and ! initial.reverse
  assert initial.filter == "" and ! initial.editing_filter
  assert ! initial.aggregate_children
  assert initial.process_memory_bytes and initial.network_auto and initial.network_sync
  assert [row.pid for row in ui.visible_processes(value, initial)] == [42, 43, 44]

  let tree_mode = press(initial, value, .Text("e"))
  assert tree_mode.tree_view
  let visible = ui.visible_tree_rows(value, tree_mode)
  assert [entry.row.pid for entry in visible] == [42, 43, 44]
  assert [entry.depth for entry in visible] == [0, 1, 2]
  assert visible[0].has_children and ! visible[0].collapsed

  let sibling_value: model.Model = model.Model(
    source_id: value.source_id,
    at_ns: value.at_ns,
    cpu_percent: value.cpu_percent,
    cpu_cores: value.cpu_cores,
    telemetry: value.telemetry,
    uptime: value.uptime,
    load: value.load,
    cpu_state: value.cpu_state,
    memory: value.memory,
    network: value.network,
    process_state: value.process_state,
    processes: [value.processes[0], value.processes[1], {...value.processes[2], parent_pid: 42}],
    storage: value.storage,
  )
  let pid_descending = ui.visible_tree_rows(sibling_value, {...tree_mode, sort: .Pid, reverse: false})
  assert [entry.row.pid for entry in pid_descending] == [42, 44, 43]
  assert [entry.depth for entry in pid_descending] == [0, 1, 1]

  let collapsed = press(tree_mode, value, .Text("-"))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, collapsed)] == [42]
  let expanded = press(collapsed, value, .Text("+"))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, expanded)] == [42, 43, 44]
  let child_toggle = press(expanded, value, .Text("C"))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, child_toggle)] == [42, 43]
  let child_expanded = press(child_toggle, value, .Text("C"))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, child_expanded)] == [42, 43, 44]
  let all_collapsed = press(child_expanded, value, .Text("E"))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, all_collapsed)] == [42, 43]
  let all_expanded = press(all_collapsed, value, .Text("E"))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, all_expanded)] == [42, 43, 44]
  let space_collapsed = press(all_expanded, value, .Text(" "))
  assert [entry.row.pid for entry in ui.visible_tree_rows(value, space_collapsed)] == [42]

  let aggregate = ui.visible_tree_rows(value, {...all_expanded, aggregate_children: true})
  assert aggregate[0].cpu_total.value == 15.0
  assert aggregate[0].rss_total_bytes == 130
  assert aggregate[1].cpu_total.value == 5.0
  assert aggregate[1].rss_total_bytes == 30

  let flat_again = press(all_expanded, value, .Text("e"))
  assert ! flat_again.tree_view
  assert [row.pid for row in ui.visible_processes(value, flat_again)] == [42, 43, 44]
}

test test_btop_ui_out_of_scope_bindings_do_not_mutate_the_view {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let value = frames[1]
  let initial = ui.initial(value, 3)
  for key in [
    "m", "p", "P", "1", "2", "3", "4", "5", "o", "?", "h", "H", "F",
    "j", "k", "l", "g", "G", "K", "u", "t", "s", "N",
    "w", "W", "v", "V", "x", "X",
  ] {
    assert press(initial, value, .Text(key)) == initial
  }
  for key in [
    input.Escape, input.Enter, input.Tab, input.Backspace, input.Delete, input.Cancel,
    input.Function(1), input.Function(2), input.Control(18), input.Control(26),
  ] {
    assert press(initial, value, key) == initial
  }
  for number in range(3, 13) {
    assert press(initial, value, .Function(number)) == initial
  }
  assert ui.reduce(initial, .Paste("n", false), value) == initial
  assert press(initial, value, .Text("C")) == initial
  assert press(initial, value, .Text("E")) == initial
  assert press(initial, value, .Text("+")) == initial
  assert press(initial, value, .Text("-")) == initial
}

test test_btop_ui_pinned_network_and_process_memory_bindings_toggle_only_their_views {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let value = frames[1]
  let initial = ui.initial(value, 3)
  assert initial.process_memory_bytes
  assert initial.network_auto
  assert initial.network_sync
  assert initial.show_disks and ! initial.disk_io_mode
  assert ! initial.process_cpu_per_core

  let manual_network = press(initial, value, .Text("a"))
  assert ! manual_network.network_auto
  assert manual_network.network_sync
  let separate_network = press(initial, value, .Text("y"))
  assert separate_network.network_auto
  assert ! separate_network.network_sync
  let percent_memory = press(initial, value, .Text("%"))
  assert ! percent_memory.process_memory_bytes
  assert percent_memory.sort == initial.sort
  assert percent_memory.selected == initial.selected

  let per_core = press(initial, value, .Text("c"))
  assert per_core.process_cpu_per_core
  assert per_core.process_memory_bytes == initial.process_memory_bytes
  assert per_core.selected == initial.selected

  let disks_hidden = press(initial, value, .Text("d"))
  assert ! disks_hidden.show_disks
  assert disks_hidden.disk_io_mode == initial.disk_io_mode
  let disk_io = press(initial, value, .Text("i"))
  assert disk_io.disk_io_mode
  assert disk_io.show_disks == initial.show_disks

  let next_network = press(initial, value, .Text("n"))
  assert next_network.network_interface == "lo"
  let previous_network = press(next_network, value, .Text("b"))
  assert previous_network.network_interface == "eth0"

  let totals_reset = press(initial, value, .Text("z"))
  let selected_network = ui.selected_network_interface(totals_reset, value)
  assert ui.network_total_bytes(totals_reset, selected_network, true) == 0
  assert ui.network_total_bytes(totals_reset, selected_network, false) == 0
  let totals_restored = press(totals_reset, value, .Text("z"))
  assert ui.network_total_bytes(totals_restored, ui.selected_network_interface(totals_restored, value), true) == 1010240
  assert ui.network_total_bytes(totals_restored, ui.selected_network_interface(totals_restored, value), false) == 502048
}

test test_btop_ui_supports_every_pinned_process_sort_and_direction {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let sampled = frames[1]
  let sampled_rows = sampled.processes |> sort-by { |row| row.pid }
  let value: model.Model = model.Model(
    source_id: sampled.source_id,
    at_ns: sampled.at_ns,
    cpu_percent: sampled.cpu_percent,
    cpu_cores: sampled.cpu_cores,
    telemetry: sampled.telemetry,
    uptime: sampled.uptime,
    load: sampled.load,
    cpu_state: sampled.cpu_state,
    memory: sampled.memory,
    network: sampled.network,
    process_state: sampled.process_state,
    processes: [
      {
        ...sampled_rows[0],
        command: "zebra",
        uid: 200,
        thread_count: 2,
        rss_bytes: 100,
        cpu_percent: {state: rates.Observed, value: 3.0},
        cpu_average_percent: {state: rates.Observed, value: 90.0},
      },
      {
        ...sampled_rows[1],
        command: "alpha",
        uid: 100,
        thread_count: 8,
        rss_bytes: 300,
        cpu_percent: {state: rates.Observed, value: 8.0},
        cpu_average_percent: {state: rates.Observed, value: 10.0},
      },
      {
        ...sampled_rows[2],
        command: "beta",
        uid: 150,
        thread_count: 4,
        rss_bytes: 200,
        cpu_percent: {state: rates.Observed, value: 5.0},
        cpu_average_percent: {state: rates.Observed, value: 50.0},
      },
    ],
    storage: sampled.storage,
  )
  let local_display: List[processes.LocalProcessDisplay] = [
    {pid: 42, start_ticks: 1000, command_line: "zebra --slow", user_name: "zed"},
    {pid: 43, start_ticks: 2000, command_line: "alpha --zzz", user_name: "amy"},
    {pid: 44, start_ticks: 3000, command_line: "beta --fast", user_name: "bea"},
  ]
  let initial = ui.initial(value, 3)

  let pid = {...initial, sort: ui.Pid}
  assert [row.pid for row in ui.visible_processes(value, pid, local_display)] == [44, 43, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, reverse: true}, local_display)] == [42, 43, 44]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.Name}, local_display)] == [43, 44, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.Name, reverse: true}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.Cmd}, local_display)] == [43, 44, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.Cmd, reverse: true}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.ThreadCount}, local_display)] == [43, 44, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.ThreadCount, reverse: true}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.UserName}, local_display)] == [43, 44, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.UserName, reverse: true}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.Memory}, local_display)] == [43, 44, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.Memory, reverse: true}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.CpuDirect}, local_display)] == [43, 44, 42]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.CpuDirect, reverse: true}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.CpuLazy}, local_display)] == [42, 44, 43]
  assert [row.pid for row in ui.visible_processes(value, {...pid, sort: ui.CpuLazy, reverse: true}, local_display)] == [43, 44, 42]
}

test test_btop_ui_filter_searches_local_command_line_and_resolved_user {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let value = frames[1]
  let local_display: List[processes.LocalProcessDisplay] = [
    {pid: 42, start_ticks: 1000, command_line: "zebra --slow", user_name: "zed"},
    {pid: 43, start_ticks: 2000, command_line: "alpha --secret-argument", user_name: "amy"},
    {pid: 44, start_ticks: 3000, command_line: "beta --fast", user_name: "bea"},
  ]
  let initial = ui.initial(value, 3)
  let by_argument = {...initial, filter: "secret-argument"}
  assert [row.pid for row in ui.visible_processes(value, by_argument, local_display)] == [43]
  let by_username = {...initial, filter: "bea"}
  assert [row.pid for row in ui.visible_processes(value, by_username, local_display)] == [44]
}

test test_btop_ui_cpu_lazy_promotes_recent_spikes_ahead_of_lifetime_average {
  let root = fs.tempdir()?
  defer root.close()
  fixture.write_b3_frame(
    root,
    "spikes",
    0,
    [
      {pid: pid, start_ticks: 1000, cpu_ticks: pid, rss_pages: 2, uid: 1000}
      for pid in range(42, 50)
    ],
  )
  let frame_root = root.open_root(p"spikes")?
  defer frame_root.close()
  let observation = source.capture(frame_root, "spikes", 1000000000, 4096, 100)?
  let sampled = model.step(observation, null)
  let value: model.Model = model.Model(
    source_id: sampled.source_id,
    at_ns: sampled.at_ns,
    cpu_percent: sampled.cpu_percent,
    cpu_cores: sampled.cpu_cores,
    telemetry: sampled.telemetry,
    uptime: sampled.uptime,
    load: sampled.load,
    cpu_state: sampled.cpu_state,
    memory: sampled.memory,
    network: sampled.network,
    process_state: sampled.process_state,
    processes: [
      {
        ...row,
        cpu_percent: {state: rates.Observed, value: if row.pid == 42 { 20.0 } else { row.pid.float() - 41.0 }},
        cpu_average_percent: {state: rates.Observed, value: row.pid.float() - 41.0},
      }
      for row in sampled.processes
    ],
    storage: sampled.storage,
  )
  let lazy = {...ui.initial(value, 8), sort: ui.CpuLazy}
  assert [row.pid for row in ui.visible_processes(value, lazy)] == [42, 49, 48, 47, 46, 45, 44, 43]
}
