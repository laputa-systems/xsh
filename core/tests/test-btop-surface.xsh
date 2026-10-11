use core.lib.btop_surface as surface
use tui

test test_btop_surface_rectangles_intersect_and_clip_by_cells {
  assert surface.intersect(
    {x: -2, y: 1, width: 8, height: 3},
    {x: 0, y: 0, width: 5, height: 8},
  ) == {x: 0, y: 1, width: 5, height: 3}
  assert surface.intersect(
    {x: 9, y: 0, width: 1, height: 1},
    {x: 0, y: 0, width: 8, height: 1},
  ) == null
  assert surface.intersect(
    {x: 0, y: 0, width: 9223372036854775807, height: 1},
    {x: 0, y: 0, width: 1, height: 1},
  ) == null

  let source = surface.from_lines(5, 2, ["A界B", "second"]) ?
  let clipped = surface.clip_rect(source, {x: 1, y: 0, width: 2, height: 2})?
  assert clipped.width == 2 and clipped.height == 2
  assert clipped.lines == ["界", "ec"]
  let crossed = surface.clip_rect(source, {x: 2, y: 0, width: 1, height: 1})?
  assert crossed.lines == [" "]
  let empty = surface.clip_rect(source, {x: 9, y: 9, width: 3, height: 2})?
  assert empty.width == 0 and empty.height == 0

  assert surface.blank(2000, 2000) is Err(_)
  assert surface.bordered(1, 4, "title", []) is Err(_)
}

test test_btop_surface_widgets_keep_geometry_and_text_inert {
  let panel = surface.bordered(8, 4, "title界", ["row\u{1b}]0;bad\u{7}", "second", "unused"])?
  assert panel.lines == [
    "+------+",
    "|title |",
    "|row ]0|",
    "+------+",
  ]
  assert tui.cell_width(panel.lines[0]) == 8
  assert tui.cell_width(panel.lines[1]) == 8
  assert tui.cell_width(panel.lines[2]) == 8
  assert tui.cell_width(panel.lines[3]) == 8

  assert surface.meter(50.0, 8) == "####----"
  assert surface.meter(-2.0, 4) == "----"
  assert surface.meter(120.0, 4) == "####"
  assert surface.meter(null, 4) == "?   "
  assert surface.table_row(["PID", "界"], [3, 2])? == "PID 界"
  assert surface.table_row(["long", "界value"], [2, 3])? == "lo 界v"
  assert surface.selected_row("界 worker", true, 8) == "> 界 wor"
  assert surface.selected_row("worker", false, 0) == ""
  assert surface.viewport(["one", "two"], 1, 3) == ["two", "", ""]
  assert surface.input_field("abcdef", 5, 4) == "cde_"
  assert surface.input_field("👩‍🔬x", 2, 3) == "👩‍🔬_"
}

test test_btop_surface_commits_only_complete_updates_and_recovers_with_full_redraw {
  let before = surface.initial()
  let first_surface = surface.from_lines(5, 2, ["abc", "right"])?
  let first = surface.prepare(before, first_surface)
  assert first.full_redraw
  assert "\u{1b}[2J" in first.bytes
  assert ! ("\n" in first.bytes)
  let committed = surface.complete(first, bytes.from_text(first.bytes).len())
  assert committed.valid
  assert committed.surface == first_surface

  let unchanged = surface.prepare(committed, first_surface)
  assert ! unchanged.full_redraw
  assert unchanged.bytes == ""
  assert surface.complete(unchanged, 0).valid

  let changed_surface = surface.from_lines(5, 2, ["ab", "right"])?
  let changed = surface.prepare(committed, changed_surface)
  assert ! changed.full_redraw
  assert "\u{1b}[1;1H" in changed.bytes
  assert "\u{1b}[K" in changed.bytes
  let partial = surface.complete(changed, bytes.from_text(changed.bytes).len() - 1)
  assert ! partial.valid
  let recovery = surface.prepare(partial, changed_surface)
  assert recovery.full_redraw
  assert "\u{1b}[2J" in recovery.bytes
  assert surface.complete(recovery, bytes.from_text(recovery.bytes).len()).valid
}

test test_btop_surface_bottom_right_frame_uses_control_sequences_without_linefeed {
  let target = surface.from_lines(4, 2, ["1234", "abcd"])?
  let update = surface.prepare(surface.initial(), target)

  assert update.bytes.ends_with("\u{1b}[2;1Habcd\u{1b}[K\u{1b}[0m")
  assert ! ("\n" in update.bytes)
  assert surface.complete(update, bytes.from_text(update.bytes).len()).valid
}
