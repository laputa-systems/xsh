use core.lib.tui_browser as browser
use core.lib.tui_input as input

test test_tui_browser_ports_fdu_window_follows_cursor_up_from_bottom {
  let first = browser.window_start(0, 99, 100, 10)
  assert first.start == 90 and first.top == 90
  assert browser.window_start(first.top, 98, 100, 10).start == 90
  assert browser.window_start(90, 90, 100, 10).start == 90
  assert browser.window_start(90, 89, 100, 10).start == 89
  assert browser.window_start(89, 95, 100, 10).start == 89
  assert browser.window_start(89, 99, 100, 10).start == 90
  assert browser.window_start(5, null, 3, 10).start == 0
  assert browser.window_start(5, null, 20, 10).start == 5
}

test test_tui_browser_ports_fdu_split_geometry {
  assert browser.fits_two_panes(browser.Vertical, 60, 1)
  assert ! browser.fits_two_panes(browser.Vertical, 59, 100)
  assert browser.fits_two_panes(browser.Horizontal, 1, 12)
  assert ! browser.fits_two_panes(browser.Horizontal, 100, 11)
  assert browser.split_toggled(browser.Vertical) == browser.Horizontal
  assert browser.split_toggled(browser.Horizontal) == browser.Vertical

  let stacked = browser.apply(browser.initial(), browser.ToggleSplit, [], 100, 30)
  assert stacked.split == browser.Horizontal
}

test test_tui_browser_focus_and_marks_control_side_visibility {
  var state = browser.reconcile(browser.initial(), [1, 2, 3])
  assert state.cursor == 1
  assert ! browser.side_visible(state, 80, 24)

  state = browser.apply(state, browser.MoveDown, [1, 2, 3], 80, 24)
  assert state.cursor == 2
  state = browser.apply(state, browser.SwitchPane, [1, 2, 3], 80, 24)
  assert state.focus == browser.Browser

  state = browser.apply(state, browser.ToggleMark, [1, 2, 3], 80, 24)
  assert state.marks == [2]
  assert state.cursor == 3
  assert browser.side_visible(state, 80, 24)

  state = browser.apply(state, browser.SwitchPane, [1, 2, 3], 80, 24)
  assert state.focus == browser.Marked
  let narrow_marked = browser.normalize(state, 50, 24)
  assert narrow_marked.focus == browser.Marked
  assert narrow_marked.side_visible

  let narrow_browser = browser.normalize({...narrow_marked, focus: browser.Browser}, 50, 24)
  assert narrow_browser.focus == browser.Browser
  assert ! narrow_browser.side_visible

  let cleared = browser.apply(narrow_marked, browser.ClearMarks, [1, 2, 3], 50, 24)
  assert cleared.focus == browser.Browser
  assert ! cleared.side_visible
}

test test_tui_browser_marks_keep_order_and_ranges_extend_with_the_cursor {
  let entries = [10, 20, 30, 40]
  var state = browser.reconcile(browser.initial(), entries)
  state = browser.toggle_mark(state, entries)
  assert state.marks == [10]
  assert state.cursor == 20
  state = browser.toggle_mark(state, entries)
  assert state.marks == [10, 20]
  assert state.cursor == 30
  state = browser.toggle_mark(state, entries)
  assert state.marks == [10, 20, 30]
  assert state.cursor == 40

  let all = browser.mark_all(browser.reconcile(browser.initial(), entries), entries)
  assert all.marks == [10, 20, 30, 40]
  assert browser.mark_all(all, entries).marks == [10, 20, 30, 40]
  assert browser.clear_marks(all).marks.is_empty()

  var ranged = browser.reconcile(browser.initial(), entries)
  ranged = browser.toggle_range(ranged)
  assert ranged.range_mode and ranged.range_anchor == 10
  assert ranged.marks == [10]
  ranged = browser.move_cursor(ranged, entries, 2)
  assert ranged.cursor == 30
  assert ranged.marks == [10, 20, 30]
  ranged = browser.toggle_range(ranged)
  assert ! ranged.range_mode and ranged.range_anchor == null
  assert ranged.marks == [10, 20, 30]

  var backward = browser.move_to_edge(browser.reconcile(browser.initial(), entries), entries, true)
  backward = browser.toggle_range(backward)
  backward = browser.move_cursor(backward, entries, -2)
  assert backward.marks == [40, 20, 30]
}

test test_tui_browser_marked_pane_moves_and_unmarks {
  let entries = [10, 20, 30]
  var state = browser.mark_all(browser.reconcile(browser.initial(), entries), entries)
  assert state.marks == [10, 20, 30]
  state = browser.move_marked_cursor(state, 2)
  assert state.marked_cursor == 2
  state = browser.unmark_at_cursor(state)
  assert state.marks == [10, 20]
  assert state.marked_cursor == 1
  state = browser.move_marked_cursor(state, 10)
  assert state.marked_cursor == 1
  state = browser.unmark_at_cursor(state)
  assert state.marks == [10]
  assert state.marked_cursor == 0
}

test test_tui_browser_intents_and_navigation_reduce_to_state {
  assert browser.browse_intent(input.Up) == browser.MoveUp
  assert browser.browse_intent(.Text("k")) == browser.MoveUp
  assert browser.browse_intent(input.Down) == browser.MoveDown
  assert browser.browse_intent(.Text("j")) == browser.MoveDown
  assert browser.browse_intent(input.PageUp) == browser.PageUp
  assert browser.browse_intent(input.PageDown) == browser.PageDown
  assert browser.browse_intent(input.Home) == browser.Home
  assert browser.browse_intent(input.End) == browser.End
  assert browser.browse_intent(input.Tab) == browser.SwitchPane
  assert browser.browse_intent(.Text("d")) == browser.ToggleMark
  assert browser.browse_intent(.Text("v")) == browser.ToggleRange
  assert browser.browse_intent(.Text("*")) == browser.MarkAll
  assert browser.browse_intent(.Text("c")) == browser.ClearMarks
  assert browser.browse_intent(.Text("-")) == browser.ToggleSplit
  assert browser.browse_intent(.Text("q")) == null

  let entries = [1, 2, 3, 4, 5]
  var state = browser.reconcile(browser.initial(), entries)
  state = browser.apply(state, browser.End, entries, 80, 24)
  assert state.cursor == 5
  state = browser.apply(state, browser.Home, entries, 80, 24)
  assert state.cursor == 1
  state = browser.apply(state, browser.PageDown, entries, 80, 2)
  assert state.cursor == 3
  state = browser.apply(state, browser.MoveDown, entries, 80, 24)
  assert state.cursor == 4
  assert browser.marker(true, false) == "[x]"
  assert browser.marker(false, true) == "[=]"
  assert browser.marker(false, false) == "   "
}

test test_tui_browser_reconcile_and_select_keep_identity {
  let entries = [1, 2, 3]
  var state = browser.reconcile(browser.initial(), entries)
  state = browser.select(state, entries, 3)
  assert state.cursor == 3
  assert browser.select(state, entries, 99).cursor == 3
  state = browser.reconcile(state, [1, 2, 3, 4])
  assert state.cursor == 3
  state = browser.reconcile(state, [1, 2, 4])
  assert state.cursor == 1
  let empty = browser.reconcile(state, [])
  assert empty.cursor == null
}
