test test_tui_helpers {
  assert tui.clear() == "\u{1b}[2J"
  assert tui.home() == "\u{1b}[H"
  assert tui.erase_line() == "\u{1b}[2K"
  assert tui.hide_cursor() == "\u{1b}[?25l"
  assert tui.show_cursor() == "\u{1b}[?25h"
  assert tui.left_pad("x", 3) == "  x"
  assert tui.right_pad("x", 3) == "x  "
  let styled = f"${tui.red()}x${tui.reset()}"
  assert styled == "\u{1b}[31mx\u{1b}[0m"
  assert tui.left_pad(styled, 3) == f"  ${styled}"
  assert tui.right_pad(styled, 3) == f"${styled}  "
  assert "\u{1b}[" in tui.reset()
  assert "\u{1b}[" in tui.red()
  assert "\u{1b}[" in tui.green()
  assert "\u{1b}[" in tui.blue()
  assert "\u{1b}[" in tui.cyan()
  assert "\u{1b}[" in tui.magenta()
  assert "\u{1b}[" in tui.yellow()
  assert "\u{1b}[" in tui.white()
  assert "\u{1b}[" in tui.gray()
  assert "\u{1b}[" in tui.bold()
  assert "\u{1b}[" in tui.dim()
}

# Every sequence producer, asserted against its exact bytes rather than a shape
# check, so a wrong or truncated selector cannot pass.
test test_tui_sequence_bytes {
  assert tui.reset() == "\u{1b}[0m"
  assert tui.bold() == "\u{1b}[1m"
  assert tui.dim() == "\u{1b}[2m"
  assert tui.red() == "\u{1b}[31m"
  assert tui.green() == "\u{1b}[32m"
  assert tui.yellow() == "\u{1b}[33m"
  assert tui.blue() == "\u{1b}[34m"
  assert tui.magenta() == "\u{1b}[35m"
  assert tui.cyan() == "\u{1b}[36m"
  assert tui.white() == "\u{1b}[37m"
  assert tui.gray() == "\u{1b}[90m"
  assert tui.clear() == "\u{1b}[2J"
  assert tui.home() == "\u{1b}[H"
  assert tui.erase_line() == "\u{1b}[2K"
  assert tui.hide_cursor() == "\u{1b}[?25l"
  assert tui.show_cursor() == "\u{1b}[?25h"
}

# Text that already reaches the requested width is returned byte-identical, in
# both directions, including when it is wider than the request.
test test_tui_pad_already_wide_enough {
  assert tui.left_pad("abcd", 4) == "abcd"
  assert tui.right_pad("abcd", 4) == "abcd"
  assert tui.left_pad("abcde", 3) == "abcde"
  assert tui.right_pad("abcde", 3) == "abcde"
  assert tui.left_pad("a", 1) == "a"
  assert tui.right_pad("a", 1) == "a"
}

# A negative width clamps to zero, and a zero width never pads: both leave the
# text unchanged, including empty text.
test test_tui_pad_zero_and_negative_width {
  assert tui.left_pad("x", 0) == "x"
  assert tui.right_pad("x", 0) == "x"
  assert tui.left_pad("x", -1) == "x"
  assert tui.right_pad("x", -1) == "x"
  assert tui.left_pad("x", -100) == "x"
  assert tui.right_pad("x", -100) == "x"
  assert tui.left_pad("", 0) == ""
  assert tui.right_pad("", 0) == ""
  assert tui.left_pad("", -5) == ""
  assert tui.right_pad("", -5) == ""
}

test test_tui_pad_large_width_keeps_space_filler {
  # The seed and each doubling boundary must keep exact output widths.
  for width in [31, 32, 33, 64, 65, 100] {
    let left = tui.left_pad("x", width)
    let right = tui.right_pad("x", width)
    assert left.byte_len() == width
    assert right.byte_len() == width
    assert left.ends_with("x")
    assert right.starts_with("x")
    assert left.replace(" ", "") == "x"
    assert right.replace(" ", "") == "x"
  }
}

# Escape sequences occupy no columns, so styled text pads to its displayed
# width and the sequences stay intact around the inserted spaces.
test test_tui_pad_ignores_escape_sequences {
  let styled = f"${tui.red()}x${tui.reset()}"
  assert tui.left_pad(styled, 1) == styled
  assert tui.left_pad(styled, 3) == f"  ${styled}"
  assert tui.right_pad(styled, 3) == f"${styled}  "
  assert tui.left_pad(f"${tui.bold()}wide${tui.reset()}", 6) == f"  ${tui.bold()}wide${tui.reset()}"

  # A value made only of escape sequences is zero columns wide.
  assert tui.left_pad(tui.reset(), 2) == f"  ${tui.reset()}"
  assert tui.right_pad(tui.clear(), 2) == f"${tui.clear()}  "
}

# Width counts Unicode scalar values, not display cells and not graphemes: a
# combining mark and an astral emoji each count as one.
test test_tui_pad_counts_unicode_scalars {
  assert tui.left_pad("héllo", 6) == " héllo"
  assert tui.right_pad("héllo", 6) == "héllo "
  assert tui.left_pad("日本", 3) == " 日本"
  assert tui.right_pad("😀", 3) == "😀  "
}

# CR and LF are zero-width, so text carrying line breaks pads on visible
# characters and keeps its line breaks in place.
test test_tui_pad_treats_cr_and_lf_as_zero_width {
  assert tui.left_pad(
    """a\r
b""",
    4,
  ) == """  a\r
b"""
  assert tui.right_pad(
    """a\r
b""",
    4,
  ) == """a\r
b  """
  assert tui.left_pad(
    """\r
""",
    0,
  ) == """\r
"""
  assert tui.left_pad(
    """\r
""",
    2,
  ) == """  \r
"""
  assert tui.right_pad(
    """\r
""",
    3,
  ) == """\r
   """
}

# An `ESC` that does not open a CSI sequence is an ordinary character and keeps
# its width; an unterminated CSI sequence runs to the end of the value and
# contributes nothing.
test test_tui_pad_lone_and_unterminated_escapes {
  assert tui.right_pad("a\u{1b}b", 3) == "a\u{1b}b"
  assert tui.right_pad("a\u{1b}", 3) == "a\u{1b} "
  assert tui.right_pad("a\u{1b}[3", 4) == "a\u{1b}[3   "
  assert tui.left_pad("a\u{1b}[31", 4) == "   a\u{1b}[31"
  assert tui.left_pad("\u{1b}[", 2) == "  \u{1b}["
  assert tui.right_pad("\u{1b}[", 2) == "\u{1b}[  "
}

test test_tui_read_secret_piped_lines { |ctx|
  let script = test.temp_file(
    ctx,
    name: "read-secret.xsh",
    contents: b"let one = tui.read_secret(\"One: \")?\nlet two = tui.read_secret(\"Two: \")?\nprint f\"${one}:${two}\"\n",
  )?

  let input = test.temp_file(ctx, name: "secret.in", contents: b"alpha\nbeta\n")?

  assert (run.text "xsh" $script < ${input}?) == """One: Two: alpha:beta
"""
}
