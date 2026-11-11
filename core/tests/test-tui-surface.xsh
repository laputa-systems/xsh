use core.lib.tui_colors as colors
use core.lib.tui_surface as surface

test test_tui_surface_ports_fdu_overlay_closing_repaints_a_wide_glyph {
  let area = {x: 0, y: 0, width: 4, height: 1}
  var listing = surface.cell_screen(4, 1)?
  listing = surface.cell_text(listing, area, "日ab", colors.PLAIN)
  var overlay = surface.cell_screen(4, 1)?
  overlay = surface.cell_text(overlay, area, "日ab", colors.PLAIN)
  overlay = surface.cell_clear(overlay, {x: 1, y: 0, width: 1, height: 1})
  overlay = surface.cell_text(overlay, {x: 1, y: 0, width: 1, height: 1}, "|", colors.PLAIN)
  assert surface.cell_plain(overlay) == " |ab"
  let restored = surface.cell_diff(overlay, listing)
  assert "日" in restored
}

test test_tui_surface_erases_both_cells_of_a_wide_glyph {
  let area = {x: 0, y: 0, width: 4, height: 1}
  var screen = surface.cell_screen(4, 1)?
  screen = surface.cell_text(screen, area, "日ab", colors.PLAIN)
  assert surface.cell_plain(screen) == "日ab"

  var continuation = surface.cell_text(screen, {x: 1, y: 0, width: 1, height: 1}, "X", colors.PLAIN)
  assert surface.cell_plain(continuation) == " Xab"
  var lead = surface.cell_text(screen, {x: 0, y: 0, width: 1, height: 1}, "X", colors.PLAIN)
  assert surface.cell_plain(lead) == "X ab"
  assert surface.cell_at(screen, 1, 0)?.text == ""
  assert surface.cell_at(screen, 4, 0) == null
}

test test_tui_surface_ports_fdu_clipping_escaping_and_size_text {
  assert surface.clip_end("directory", 6) == "direc…"
  assert surface.clip_start("a/b/long-name", 7) == "…g-name"
  assert surface.clip_end("short", 10) == "short"
  assert surface.clip_end("日本語日本語", 6) == "日本…"
  assert surface.clip_start("日本語日本語", 6) == "…本語"
  assert surface.clip_end("e\u{301}e\u{301}e\u{301}", 2) == "e\u{301}…"
  assert surface.clip_start("e\u{301}e\u{301}e\u{301}", 2) == "…e\u{301}"
  assert surface.clip_end("anything", 0) == ""

  assert surface.escape_name(b"safe\n\x1b-\xff\xc3\xa9") == "safe\\u{a}\\u{1b}-\\xffé"
  assert surface.pad_to("a", 3) == "a  "
  assert surface.pad_to("abc", 2) == "abc"

  assert surface.format_size(512) == "512 B"
  assert surface.format_size(1024) == "1.0 KiB"
  assert surface.format_size(4096) == "4.0 KiB"
  assert surface.format_size(5 * 1024 * 1024) == "5.0 MiB"
}

test test_tui_surface_ports_fdu_age_rounding_and_unknown_times {
  assert surface.format_age_seconds(365 * 24 * 60 * 60) == "1y"
  assert surface.format_age_seconds(2 * 7 * 24 * 60 * 60) == "2w"
  assert surface.format_age_seconds(4 * 24 * 60 * 60) == "4d"
  assert surface.format_age_seconds(5 * 60) == "5m"
  assert surface.format_age_seconds(5 * 60 + 22) == "5m"
  assert surface.format_age_seconds(60 * 60 + 2 * 60) == "1h"
  assert surface.format_age_seconds(60 * 60 + 30 * 60) == "2h"
  assert surface.format_age_seconds(31 * 24 * 60 * 60) == "1mo"
  assert surface.format_age_seconds(20 * (365 * 24 * 60 * 60 / 12)) == "2y"
  assert surface.format_age_seconds(1000 * 365 * 24 * 60 * 60) == "999y+"
  assert surface.format_age_seconds(8) == "8s"

  assert surface.format_age(null, 2000000000) == "—"
  assert surface.format_age(2000000030, 2000000000) == "0s"
  assert surface.format_age(2000000000 - 120, 2000000000) == "2m"
}

test test_tui_surface_splits_rectangles_at_rounded_percentages {
  let columns = surface.split_columns({x: 2, y: 3, width: 10, height: 4}, 55)
  assert columns[0] == {x: 2, y: 3, width: 6, height: 4}
  assert columns[1] == {x: 8, y: 3, width: 4, height: 4}
  let narrow = surface.split_columns({x: 0, y: 0, width: 7, height: 1}, 50)
  assert narrow[0].width == 4 and narrow[1].width == 3

  let rows = surface.split_rows({x: 0, y: 0, width: 4, height: 10}, 60)
  assert rows[0] == {x: 0, y: 0, width: 4, height: 6}
  assert rows[1] == {x: 0, y: 6, width: 4, height: 4}
  let short = surface.split_rows({x: 0, y: 0, width: 4, height: 5}, 60)
  assert short[0].height == 3 and short[1].height == 2
}

test test_tui_surface_modal_draws_at_every_supported_size {
  for columns in [1, 5, 12, 36, 59, 60, 100, 140] {
    for height in [1, 3, 8, 30] {
      let screen = surface.cell_screen(columns, height)?
      let drawn = surface.cell_modal(screen, "Help", ["Type a fragment", "/ abc", "Esc cancels"])?
      assert drawn.width == columns and drawn.height == height
      let centered = surface.modal_area({x: 0, y: 0, width: columns, height: height}, 94, height)
      assert centered.width >= 1 and centered.width <= columns
      assert centered.height <= height
    }
  }
  let empty = surface.cell_modal(surface.cell_screen(0, 0)?, "Help", [])?
  assert empty.width == 0 and empty.height == 0
}

test test_tui_surface_wide_combining_and_control_names_stay_in_the_pane {
  let bordered = surface.cell_border(
    surface.cell_screen(44, 10)?,
    {x: 0, y: 0, width: 44, height: 10},
    "Names",
    colors.PLAIN,
    colors.BOLD,
  )
  let names = [
    surface.escape_name(b"\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e-wide-name-that-is-long"),
    surface.escape_name(b"e\xcc\x81e\xcc\x81-combining"),
    surface.escape_name(b"tab\tname\xff"),
  ]
  let filled = surface.cell_lines(bordered.screen, bordered.inner, names)
  let text = surface.cell_plain(filled)
  assert "tab\\u{9}name\\xff" in text
  let rows = text.split("\n")
  for row in rows[2..5] {
    assert row.ends_with("│")
  }
}

test test_tui_surface_cell_diff_emits_styles_and_only_changes {
  let blank = surface.cell_screen(3, 1)?
  let red = surface.cell_text(blank, {x: 0, y: 0, width: 3, height: 1}, "abc", colors.RED)
  let emitted = surface.cell_diff(blank, red)
  assert "\u{1b}[0;31m" in emitted
  assert emitted.ends_with("\u{1b}[0m\u{1b}[1;1H")
  assert surface.cell_diff(red, red) == ""

  let moved = surface.cell_text(blank, {x: 1, y: 0, width: 2, height: 1}, "bc", colors.PLAIN)
  let delta = surface.cell_diff(red, moved)
  assert "bc" in delta
  assert ! ("a" in delta)
}
