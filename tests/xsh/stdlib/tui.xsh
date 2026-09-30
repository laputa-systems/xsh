test test_tui_helpers [error] {
  tui.clear() == "\u{1b}[2J"
  tui.home() == "\u{1b}[H"
  tui.erase_line() == "\u{1b}[2K"
  tui.hide_cursor() == "\u{1b}[?25l"
  tui.show_cursor() == "\u{1b}[?25h"
  tui.left_pad("x", 3) == "  x"
  tui.right_pad("x", 3) == "x  "
  let styled = f"${tui.red()}x${tui.reset()}"
  styled == "\u{1b}[31mx\u{1b}[0m"
  tui.left_pad(styled, 3) == f"  ${styled}"
  tui.right_pad(styled, 3) == f"${styled}  "
  ("\u{1b}[" in tui.reset())
  ("\u{1b}[" in tui.red())
  ("\u{1b}[" in tui.green())
  ("\u{1b}[" in tui.blue())
  ("\u{1b}[" in tui.cyan())
  ("\u{1b}[" in tui.magenta())
  ("\u{1b}[" in tui.yellow())
  ("\u{1b}[" in tui.white())
  ("\u{1b}[" in tui.gray())
  ("\u{1b}[" in tui.bold())
  ("\u{1b}[" in tui.dim())
}

# Every sequence producer, asserted against its exact bytes rather than a shape
# check, so a wrong or truncated selector cannot pass.
test test_tui_sequence_bytes [error] {
  tui.reset() == "\u{1b}[0m"
  tui.bold() == "\u{1b}[1m"
  tui.dim() == "\u{1b}[2m"
  tui.red() == "\u{1b}[31m"
  tui.green() == "\u{1b}[32m"
  tui.yellow() == "\u{1b}[33m"
  tui.blue() == "\u{1b}[34m"
  tui.magenta() == "\u{1b}[35m"
  tui.cyan() == "\u{1b}[36m"
  tui.white() == "\u{1b}[37m"
  tui.gray() == "\u{1b}[90m"
  tui.clear() == "\u{1b}[2J"
  tui.home() == "\u{1b}[H"
  tui.erase_line() == "\u{1b}[2K"
  tui.hide_cursor() == "\u{1b}[?25l"
  tui.show_cursor() == "\u{1b}[?25h"
}

# Text that already reaches the requested width is returned byte-identical, in
# both directions, including when it is wider than the request.
test test_tui_pad_already_wide_enough [error] {
  tui.left_pad("abcd", 4) == "abcd"
  tui.right_pad("abcd", 4) == "abcd"
  tui.left_pad("abcde", 3) == "abcde"
  tui.right_pad("abcde", 3) == "abcde"
  tui.left_pad("a", 1) == "a"
  tui.right_pad("a", 1) == "a"
}

# A negative width clamps to zero, and a zero width never pads: both leave the
# text unchanged, including empty text.
test test_tui_pad_zero_and_negative_width [error] {
  tui.left_pad("x", 0) == "x"
  tui.right_pad("x", 0) == "x"
  tui.left_pad("x", -1) == "x"
  tui.right_pad("x", -1) == "x"
  tui.left_pad("x", -100) == "x"
  tui.right_pad("x", -100) == "x"
  tui.left_pad("", 0) == ""
  tui.right_pad("", 0) == ""
  tui.left_pad("", -5) == ""
  tui.right_pad("", -5) == ""
}

test test_tui_pad_large_width_keeps_space_filler [error] {
  # The seed and each doubling boundary must keep exact output widths.
  for width in [31, 32, 33, 64, 65, 100] {
    let left = tui.left_pad("x", width)
    let right = tui.right_pad("x", width)
    left.byte_len() == width
    right.byte_len() == width
    left.ends_with("x")
    right.starts_with("x")
    left.replace(" ", "") == "x"
    right.replace(" ", "") == "x"
  }
}

# Escape sequences occupy no columns, so styled text pads to its displayed
# width and the sequences stay intact around the inserted spaces.
test test_tui_pad_ignores_escape_sequences [error] {
  let styled = f"${tui.red()}x${tui.reset()}"
  tui.left_pad(styled, 1) == styled
  tui.left_pad(styled, 3) == f"  ${styled}"
  tui.right_pad(styled, 3) == f"${styled}  "
  tui.left_pad(f"${tui.bold()}wide${tui.reset()}", 6) == f"  ${tui.bold()}wide${tui.reset()}"

  # A value made only of escape sequences is zero columns wide.
  tui.left_pad(tui.reset(), 2) == f"  ${tui.reset()}"
  tui.right_pad(tui.clear(), 2) == f"${tui.clear()}  "
}

# Width counts Unicode scalar values, not display cells and not graphemes: a
# combining mark and an astral emoji each count as one.
test test_tui_pad_counts_unicode_scalars [error] {
  tui.left_pad("h\u{e9}llo", 6) == " h\u{e9}llo"
  tui.right_pad("h\u{e9}llo", 6) == "h\u{e9}llo "
  tui.left_pad("\u{65e5}\u{672c}", 3) == " \u{65e5}\u{672c}"
  tui.right_pad("\u{1f600}", 3) == "\u{1f600}  "
}

# CR and LF are zero-width, so text carrying line breaks pads on visible
# characters and keeps its line breaks in place.
test test_tui_pad_treats_cr_and_lf_as_zero_width [error] {
  tui.left_pad(
  """a\r
b""",
  4,
) == """  a\r
b"""
  tui.right_pad(
  """a\r
b""",
  4,
) == """a\r
b  """
  tui.left_pad(
  """\r
""",
  0,
) == """\r
"""
  tui.left_pad(
  """\r
""",
  2,
) == """  \r
"""
  tui.right_pad(
  """\r
""",
  3,
) == """\r
   """
}

# An `ESC` that does not open a CSI sequence is an ordinary character and keeps
# its width; an unterminated CSI sequence runs to the end of the value and
# contributes nothing.
test test_tui_pad_lone_and_unterminated_escapes [error] {
  tui.right_pad("a\u{1b}b", 3) == "a\u{1b}b"
  tui.right_pad("a\u{1b}", 3) == "a\u{1b} "
  tui.right_pad("a\u{1b}[3", 4) == "a\u{1b}[3   "
  tui.left_pad("a\u{1b}[31", 4) == "   a\u{1b}[31"
  tui.left_pad("\u{1b}[", 2) == "  \u{1b}["
  tui.right_pad("\u{1b}[", 2) == "\u{1b}[  "
}

test test_tui_read_secret_piped_lines [fs, process, error] { |ctx|
  let script = test.temp_file(
    ctx,
    name: "read-secret.xsh",
    contents: b"let one = tui.read_secret(\"One: \")?\nlet two = tui.read_secret(\"Two: \")?\nprint f\"${one}:${two}\"\n",
  )?

  let input = test.temp_file(ctx, name: "secret.in", contents: b"alpha\nbeta\n")?

  (run.text "xsh" $script < ${input}?) == """One: Two: alpha:beta
"""
}
