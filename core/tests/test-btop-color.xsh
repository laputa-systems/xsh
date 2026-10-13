use core.lib.btop_color as color
use core.lib.btop_surface as surface
use core.lib.btop_terminal as terminal

test test_btop_palette_maps_fixed_truecolor_roles_to_supported_depths {
  assert color.truecolor(color.Cpu) == {red: 0, green: 255, blue: 0}
  assert color.foreground(color.Cpu, terminal.TrueColor) == "\u{1b}[38;2;0;255;0m"
  assert color.foreground(color.Cpu, terminal.Ansi256) == "\u{1b}[38;5;46m"
  assert color.foreground(color.Cpu, terminal.Ansi16) == "\u{1b}[92m"
  assert color.foreground(color.Memory, terminal.Ansi16) == "\u{1b}[36m"
  assert color.foreground(color.Network, terminal.Ansi16) == "\u{1b}[93m"
  assert color.foreground(color.Cpu, terminal.NoColor) == ""

  let no_color = terminal.detect({term: "xterm-256color", colorterm: "", no_color: true, lc_all: "C.UTF-8", lc_ctype: "", lang: "", rows: 24, cols: 80})
  assert no_color.color_depth == .NoColor
  assert color.foreground(color.Cpu, no_color.color_depth) == ""
}

test test_btop_surface_commits_palette_roles_and_resets_each_colored_row {
  let colored = surface.from_lines_with_roles(5, 2, ["cpu", "mem"], [color.Cpu, color.Memory])?
  let first = surface.prepare(surface.initial(), colored, terminal.TrueColor)
  assert "\u{1b}[38;2;0;255;0m" in first.bytes
  assert "\u{1b}[38;2;0;128;255m" in first.bytes
  assert first.bytes.split("\u{1b}[0m").len() == 3

  let committed = surface.complete(first, bytes.from_text(first.bytes).len())
  let unchanged = surface.prepare(committed, colored, terminal.TrueColor)
  assert unchanged.bytes == ""
  assert ! unchanged.full_redraw

  let changed_role = surface.from_lines_with_roles(5, 2, ["cpu", "mem"], [color.Network, color.Memory])?
  let update = surface.prepare(committed, changed_role, terminal.TrueColor)
  assert "\u{1b}[1;1H\u{1b}[38;2;255;170;0m" in update.bytes

  let uncolored = surface.prepare(committed, colored, terminal.NoColor)
  assert uncolored.full_redraw
  assert ! ("\u{1b}[38;" in uncolored.bytes)
}
