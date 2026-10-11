use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_source as source
use core.lib.btop_ui as ui
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
  ui.reduce(state, input.Pressed(key), value)
}

test test_btop_ui_keeps_selection_by_process_lifetime_across_sort_and_pid_reuse {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let first = frames[1]
  let second = frames[2]
  let initial = ui.initial(first, 1)

  assert [row.pid for row in ui.visible_processes(first, initial)] == [43, 44, 42]
  assert initial.selected == {pid: 43, start_ticks: 2000}
  let after_pid_reuse = ui.refresh(initial, second)
  assert after_pid_reuse.selected == {pid: 44, start_ticks: 3000}
  assert after_pid_reuse.selected != {pid: 43, start_ticks: 2000}
  let selected = press(initial, first, input.Down)
  assert selected.selected == {pid: 44, start_ticks: 3000}
  assert selected.scroll == 1

  let reversed = press(selected, first, input.Text("r"))
  assert reversed.reverse
  assert [row.pid for row in ui.visible_processes(first, reversed)] == [42, 44, 43]
  assert reversed.selected == selected.selected

  let by_pid = press(reversed, first, input.Right)
  assert by_pid.sort == ui.Pid
  assert [row.pid for row in ui.visible_processes(first, by_pid)] == [44, 43, 42]
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

  let editing = press(initial, value, input.Text("f"))
  assert editing.editing_filter
  let typed = ui.reduce_events(editing, [
    input.Pressed(input.Text("4")),
    input.Pressed(input.Text("2")),
    input.Pressed(input.Text("q")),
    input.Pressed(input.Backspace),
  ], value)
  assert typed.filter_draft == "42"
  assert typed.filter == ""
  let accepted = press(typed, value, input.Enter)
  assert ! accepted.editing_filter
  assert accepted.filter == "42"
  assert [row.pid for row in ui.visible_processes(value, accepted)] == [42]

  let edit_again = press(accepted, value, input.Text("/"))
  let canceled = press(press(edit_again, value, input.Text("x")), value, input.Escape)
  assert ! canceled.editing_filter
  assert canceled.filter == "42"
  assert canceled.filter_draft == ""

  let clear = press(canceled, value, input.Delete)
  assert clear.filter == ""
  assert ui.visible_processes(value, clear).len() == 3

  let editing_again = press(clear, value, input.Text("f"))
  let pasted = ui.reduce(editing_again, input.Paste("q\r\n", false), value)
  assert pasted == editing_again
}

test test_btop_ui_resize_preserves_identity_and_clamps_scroll {
  let root = fs.tempdir()?
  defer root.close()
  let frames = prepare_frames(root)?
  let value = frames[1]
  let initial = ui.initial(value, 1)
  let selected = press(press(initial, value, input.Down), value, input.Down)
  assert selected.selected == {pid: 42, start_ticks: 1000}
  assert selected.scroll == 2

  let larger = ui.resize(selected, value, 3)
  assert larger.selected == selected.selected
  assert larger.scroll == 0
  assert larger.visible_rows == 3

  let filtered = press(press(larger, value, input.Text("f")), value, input.Text("missing"))
  let accepted = press(filtered, value, input.Enter)
  assert accepted.selected == null
  assert accepted.scroll == 0
  assert ui.selection_position(accepted, value) == null
}
