use core.lib.btop_render as render
use core.lib.btop_replay as replay
use core.lib.btop_graphs as graphs
use core.lib.btop_ui as ui
use core.lib.btop_terminal as terminal
use core.lib.btop_color as color
use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.tests.support.btop as fixture
use tui

test test_btop_g1_screen_places_all_panel_values_at_80_by_24 {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let screen = render.render(frames[1], 80, 24)?

  assert ! screen.too_small
  assert screen.lines.len() == 24
  assert "CPU" in screen.lines[0]
  assert "Usage 40.0%" in screen.lines[1]
  assert "Temperature absent" in screen.lines[5]
  assert "Frequency absent" in screen.lines[5]
  assert "Battery absent" in screen.lines[6]
  assert "Total 1.00 GiB" in screen.lines[12]
  assert "Used 75.0%" in screen.lines[9]
  assert "Interface eth0" in screen.lines[18]
  assert "RX 10.00 KiB/s" in screen.lines[19]
  assert "TX 2.00 KiB/s" in screen.lines[21]
  assert "PID USER CPU% RSS COMMAND" in (screen.lines[9].split("") |> drop(44).join(""))
  assert "Sort CPU lazy descending" in (screen.lines[10].split("") |> drop(44).join(""))
  assert "> 42 1000 12.5% 64.00 MiB" in (screen.lines[11].split("") |> drop(44).join(""))
}

test test_btop_g1_screen_places_all_panel_values_at_120_by_40 {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let screen = render.render(frames[1], 120, 40)?

  assert ! screen.too_small
  assert screen.lines.len() == 40
  assert "Usage 40.0%" in screen.lines[1]
  assert "Total 1.00 GiB" in screen.lines[17]
  assert "RX 10.00 KiB/s" in screen.lines[31]
  assert "TX 2.00 KiB/s" in screen.lines[33]
  assert "42 1000 12.5%" in (screen.lines[16].split("") |> drop(66).join(""))
}

test test_btop_g1_screen_reports_safe_minimum_dimensions {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, [])
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let screen = render.render(frames[1], 79, 24)?

  assert screen.too_small
  assert screen.lines.len() == 1
  assert "terminal too small (minimum 80x24)" in screen.lines[0]
  assert screen.lines[0].count_chars() <= 79
}

test test_btop_g4a_default_panels_use_upstream_relative_geometry_and_memory_disks_section {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, [])
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let screen = render.render_view_with_graphs(
    frames[1],
    ui.initial(frames[1], render.process_capacity(24)),
    80,
    24,
    [],
    graphs.empty(),
    terminal.detect({term: "xterm-256color", colorterm: "", no_color: false, lc_all: "C.UTF-8", lc_ctype: "", lang: "", rows: 24, cols: 80}),
  )?

  assert screen.lines.len() == 24
  assert screen.lines[0].starts_with("╭") and screen.lines[0].ends_with("╮")
  assert screen.lines[0].count_chars() == 80
  assert screen.row_roles[0] == color.Cpu
  assert "CPU" in screen.lines[0]
  assert screen.lines[8].starts_with("╭") and screen.lines[8].ends_with("╮")
  assert "Memory / Disks" in screen.lines[8]
  assert screen.lines[8].split("")[44].starts_with("╭")
  assert screen.lines[17].starts_with("╭") and "Network" in screen.lines[17]
  assert screen.lines[17].split("")[44] != "╭"
  assert "Disks unavailable" in screen.lines.join("\n")
  assert screen.row_roles[8] == color.Memory
  assert screen.row_roles[17] == color.Network
  assert "Processes" in (screen.lines[8].split("") |> drop(44).join(""))
}

test test_btop_g4a_cpu_memory_and_network_graphs_render_retained_history {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let first = graphs.observe(graphs.empty(), frames[0])?
  let history = graphs.observe(first, frames[1])?
  let screen = render.render_view_with_graphs(
    frames[1],
    ui.initial(frames[1], render.process_capacity(40)),
    120,
    40,
    [],
    history,
    terminal.detect({term: "xterm-256color", colorterm: "", no_color: false, lc_all: "C.UTF-8", lc_ctype: "", lang: "", rows: 40, cols: 120}),
  )?

  assert screen.lines.len() == 40
  assert tui.cell_width(screen.lines[6]) == 120
  assert "\u{28b8}" in screen.lines[6]
  assert "Memory / Disks" in screen.lines[13]
  assert "Network" in screen.lines[29]
  assert "RX 10.00 KiB/s" in screen.lines.join("\n")
  assert "TX 2.00 KiB/s" in screen.lines.join("\n")
}

test test_btop_g4a_process_tree_golden_shows_depth_collapse_and_aggregation {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [
      {pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000},
      {pid: 43, start_ticks: 2000, cpu_ticks: 20, rss_pages: 20, uid: 1000},
      {pid: 44, start_ticks: 3000, cpu_ticks: 30, rss_pages: 10, uid: 1000},
    ],
  )
  let loaded = replay.load(bundle.host_path()?)?
  let base = replay.models(loaded.observations)[1]
  let rows = base.processes |> sort-by { |row| row.pid }
  let value: model.Model = {
    ...base,
    processes: [
      {...rows[0], parent_pid: 0, cpu_percent: {state: rates.Observed, value: 10.0}, cpu_average_percent: {state: rates.Observed, value: 10.0}, rss_bytes: 100},
      {...rows[1], parent_pid: 42, cpu_percent: {state: rates.Observed, value: 3.0}, cpu_average_percent: {state: rates.Observed, value: 3.0}, rss_bytes: 20},
      {...rows[2], parent_pid: 43, cpu_percent: {state: rates.Observed, value: 2.0}, cpu_average_percent: {state: rates.Observed, value: 2.0}, rss_bytes: 10},
    ],
  }
  let initial = ui.initial(value, render.process_capacity(24))
  let tree_mode = ui.reduce(initial, input.Pressed(input.Text("e")), value)
  let tree_screen = render.render_view(value, tree_mode, 80, 24, [])?
  let parent_line = tree_screen.lines[11].split("") |> drop(45) |> join("")
  let child_line = tree_screen.lines[12].split("") |> drop(45) |> join("")
  let grandchild_line = tree_screen.lines[13].split("") |> drop(45) |> join("")
  assert parent_line.starts_with("> [-] 42 1000 10.0% 100 B"), parent_line
  assert child_line.starts_with("    [-] 43 1000 3.0% 20 B")
  assert grandchild_line.starts_with("      └ 44 1000 2.0% 10 B")

  let aggregate_state = {...tree_mode, aggregate_children: true}
  let aggregate_screen = render.render_view(value, aggregate_state, 80, 24, [])?
  let aggregate_parent = aggregate_screen.lines[11].split("") |> drop(45) |> join("")
  let aggregate_child = aggregate_screen.lines[12].split("") |> drop(45) |> join("")
  assert aggregate_parent.starts_with("> [-] 42 1000 15.0% 130 B")
  assert aggregate_child.starts_with("    [-] 43 1000 5.0% 30 B")

  let collapsed = ui.reduce(tree_mode, input.Pressed(input.Text("-")), value)
  let collapsed_screen = render.render_view(value, collapsed, 80, 24, [])?
  let collapsed_parent = collapsed_screen.lines[11].split("") |> drop(45) |> join("")
  assert collapsed_parent.starts_with("> [+] 42 1000 10.0% 100 B")
  assert ! ("44 1000" in collapsed_screen.lines[11..].join("\n"))
}
