use core.lib.tui_colors as colors

test test_tui_colors_ports_fdu_default_type_colors {
  let parsed = colors.default_ls_colors()
  assert colors.style_for(parsed, b"projects", .Directory) == {fg: colors.Blue, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(parsed, b"link", .Symlink) == {fg: colors.Cyan, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(parsed, b"file", .Regular) == colors.PLAIN
}

test test_tui_colors_ports_fdu_type_and_extension_precedence {
  let parsed = colors.parse_ls_colors("di=01;34:ln=01;36:fi=0:*.tar=01;31:*.gz=01;35:")
  assert colors.style_for(parsed, b"dir", .Directory) == {fg: colors.Blue, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(parsed, b"plain", .Regular) == colors.PLAIN
  assert colors.style_for(parsed, b"archive.tar", .Regular) == {fg: colors.Red, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(parsed, b"archive.tgz", .Regular) == colors.PLAIN

  let typed = colors.parse_ls_colors("di=01;34:*.tar=01;31:")
  assert colors.style_for(typed, b"archive.tar", .Directory) == {fg: colors.Blue, bg: null, bold: true, dim: false, reversed: false}
}

test test_tui_colors_ports_fdu_extended_bright_and_malformed_values {
  let parsed = colors.parse_ls_colors("di=38;5;27:ln=38;2;0;255;255:fi=90:")
  assert colors.style_for(parsed, b"d", .Directory) == {fg: colors.Ansi256(27), bg: null, bold: false, dim: false, reversed: false}
  assert colors.style_for(parsed, b"l", .Symlink) == {fg: colors.Rgb(0, 255, 255), bg: null, bold: false, dim: false, reversed: false}
  assert colors.style_for(parsed, b"f", .Regular) == {fg: colors.BrightBlack, bg: null, bold: false, dim: false, reversed: false}

  let broken = colors.parse_ls_colors("di=:no-equals:ln=target:fi=01;not-a-number:")
  assert colors.style_for(broken, b"d", .Directory) == {fg: colors.Blue, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(broken, b"l", .Symlink) == {fg: colors.Cyan, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(broken, b"f", .Regular) == {fg: null, bg: null, bold: true, dim: false, reversed: false}
}

test test_tui_colors_ports_fdu_glob_matching_order_and_case {
  let parsed = colors.parse_ls_colors("*.tar.gz=01;31:*.tar=01;34:*README=01;33:")
  let gzip = {fg: colors.Red, bg: null, bold: true, dim: false, reversed: false}
  let tar = {fg: colors.Blue, bg: null, bold: true, dim: false, reversed: false}
  let readme = {fg: colors.Yellow, bg: null, bold: true, dim: false, reversed: false}
  assert colors.style_for(parsed, b"archive.tar.gz", .Regular) == gzip
  assert colors.style_for(parsed, b"archive.tar", .Regular) == tar
  assert colors.style_for(parsed, b"archive.TAR.GZ", .Regular) == colors.PLAIN
  assert colors.style_for(parsed, b"tar", .Regular) == colors.PLAIN
  assert colors.style_for(parsed, b"README", .Regular) == readme
  assert colors.style_for(parsed, b"docs-README", .Regular) == readme
  assert colors.style_for(parsed, b"docs-README.md", .Regular) == colors.PLAIN
}

test test_tui_colors_renders_complete_sgr_and_cursor_highlight {
  assert colors.ansi(colors.PLAIN) == "\u{1b}[0m"
  assert colors.ansi(colors.RED) == "\u{1b}[0;31m"
  assert colors.ansi({fg: colors.Cyan, bg: colors.Black, bold: true, dim: true, reversed: true}) == "\u{1b}[0;1;2;7;36;40m"
  assert colors.ansi({fg: colors.Ansi256(27), bg: null, bold: false, dim: false, reversed: false}) == "\u{1b}[0;38;5;27m"
  assert colors.ansi({fg: null, bg: colors.Rgb(1, 2, 3), bold: false, dim: false, reversed: false}) == "\u{1b}[0;48;2;1;2;3m"

  let focused = colors.with_cursor(colors.YELLOW, true)
  assert focused.fg == colors.Yellow and focused.reversed and ! focused.bold
  let unfocused = colors.with_cursor(colors.YELLOW, false)
  assert unfocused.fg == colors.Yellow and unfocused.bold and ! unfocused.reversed
}
