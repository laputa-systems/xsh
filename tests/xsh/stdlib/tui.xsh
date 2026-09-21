proc test_tui_helpers() [error] {
  test.eq(tui.clear(), "\u{1b}[2J")?
  test.eq(tui.home(), "\u{1b}[H")?
  test.eq(tui.erase_line(), "\u{1b}[2K")?
  test.eq(tui.hide_cursor(), "\u{1b}[?25l")?
  test.eq(tui.show_cursor(), "\u{1b}[?25h")?
  test.eq(tui.left_pad("x", 3), "  x")?
  test.eq(tui.right_pad("x", 3), "x  ")?
  let styled = f"${tui.red()}x${tui.reset()}"
  test.eq(styled, "\u{1b}[31mx\u{1b}[0m")?
  test.eq(tui.left_pad(styled, 3), f"  ${styled}")?
  test.eq(tui.right_pad(styled, 3), f"${styled}  ")?
  test.ok(tui.reset().contains("\u{1b}["))?
  test.ok(tui.red().contains("\u{1b}["))?
  test.ok(tui.green().contains("\u{1b}["))?
  test.ok(tui.blue().contains("\u{1b}["))?
  test.ok(tui.cyan().contains("\u{1b}["))?
  test.ok(tui.magenta().contains("\u{1b}["))?
  test.ok(tui.yellow().contains("\u{1b}["))?
  test.ok(tui.white().contains("\u{1b}["))?
  test.ok(tui.gray().contains("\u{1b}["))?
  test.ok(tui.bold().contains("\u{1b}["))?
  test.ok(tui.dim().contains("\u{1b}["))?
}

# Every sequence producer, asserted against its exact bytes rather than a shape
# check, so a wrong or truncated selector cannot pass.
proc test_tui_sequence_bytes() [error] {
  test.eq(tui.reset(), "\u{1b}[0m")?
  test.eq(tui.bold(), "\u{1b}[1m")?
  test.eq(tui.dim(), "\u{1b}[2m")?
  test.eq(tui.red(), "\u{1b}[31m")?
  test.eq(tui.green(), "\u{1b}[32m")?
  test.eq(tui.yellow(), "\u{1b}[33m")?
  test.eq(tui.blue(), "\u{1b}[34m")?
  test.eq(tui.magenta(), "\u{1b}[35m")?
  test.eq(tui.cyan(), "\u{1b}[36m")?
  test.eq(tui.white(), "\u{1b}[37m")?
  test.eq(tui.gray(), "\u{1b}[90m")?
  test.eq(tui.clear(), "\u{1b}[2J")?
  test.eq(tui.home(), "\u{1b}[H")?
  test.eq(tui.erase_line(), "\u{1b}[2K")?
  test.eq(tui.hide_cursor(), "\u{1b}[?25l")?
  test.eq(tui.show_cursor(), "\u{1b}[?25h")?
}

# Text that already reaches the requested width is returned byte-identical, in
# both directions, including when it is wider than the request.
proc test_tui_pad_already_wide_enough() [error] {
  test.eq(tui.left_pad("abcd", 4), "abcd")?
  test.eq(tui.right_pad("abcd", 4), "abcd")?
  test.eq(tui.left_pad("abcde", 3), "abcde")?
  test.eq(tui.right_pad("abcde", 3), "abcde")?
  test.eq(tui.left_pad("a", 1), "a")?
  test.eq(tui.right_pad("a", 1), "a")?
}

# A negative width clamps to zero, and a zero width never pads: both leave the
# text unchanged, including empty text.
proc test_tui_pad_zero_and_negative_width() [error] {
  test.eq(tui.left_pad("x", 0), "x")?
  test.eq(tui.right_pad("x", 0), "x")?
  test.eq(tui.left_pad("x", -1), "x")?
  test.eq(tui.right_pad("x", -1), "x")?
  test.eq(tui.left_pad("x", -100), "x")?
  test.eq(tui.right_pad("x", -100), "x")?
  test.eq(tui.left_pad("", 0), "")?
  test.eq(tui.right_pad("", 0), "")?
  test.eq(tui.left_pad("", -5), "")?
  test.eq(tui.right_pad("", -5), "")?
}

# Escape sequences occupy no columns, so styled text pads to its displayed
# width and the sequences stay intact around the inserted spaces.
proc test_tui_pad_ignores_escape_sequences() [error] {
  let styled = f"${tui.red()}x${tui.reset()}"
  test.eq(tui.left_pad(styled, 1), styled)?
  test.eq(tui.left_pad(styled, 3), f"  ${styled}")?
  test.eq(tui.right_pad(styled, 3), f"${styled}  ")?
  test.eq(
    tui.left_pad(f"${tui.bold()}wide${tui.reset()}", 6),
    f"  ${tui.bold()}wide${tui.reset()}",
  )?

  # A value made only of escape sequences is zero columns wide.
  test.eq(tui.left_pad(tui.reset(), 2), f"  ${tui.reset()}")?
  test.eq(tui.right_pad(tui.clear(), 2), f"${tui.clear()}  ")?
}

# Width counts Unicode scalar values, not display cells and not graphemes: a
# combining mark and an astral emoji each count as one.
proc test_tui_pad_counts_unicode_scalars() [error] {
  test.eq(tui.left_pad("h\u{e9}llo", 6), " h\u{e9}llo")?
  test.eq(tui.right_pad("h\u{e9}llo", 6), "h\u{e9}llo ")?
  test.eq(tui.left_pad("\u{65e5}\u{672c}", 3), " \u{65e5}\u{672c}")?
  test.eq(tui.right_pad("\u{1f600}", 3), "\u{1f600}  ")?
}

# CR and LF are zero-width, so text carrying line breaks pads on visible
# characters and keeps its line breaks in place.
proc test_tui_pad_treats_cr_and_lf_as_zero_width() [error] {
  test.eq(
    tui.left_pad(
  """a\r
b""",
  4,
),
    """  a\r
b""",
  )?
  test.eq(
    tui.right_pad(
  """a\r
b""",
  4,
),
    """a\r
b  """,
  )?
  test.eq(
    tui.left_pad(
  """\r
""",
  0,
),
    """\r
""",
  )?
  test.eq(
    tui.left_pad(
  """\r
""",
  2,
),
    """  \r
""",
  )?
  test.eq(
    tui.right_pad(
  """\r
""",
  3,
),
    """\r
   """,
  )?
}

# An `ESC` that does not open a CSI sequence is an ordinary character and keeps
# its width; an unterminated CSI sequence runs to the end of the value and
# contributes nothing.
proc test_tui_pad_lone_and_unterminated_escapes() [error] {
  test.eq(tui.right_pad("a\u{1b}b", 3), "a\u{1b}b")?
  test.eq(tui.right_pad("a\u{1b}", 3), "a\u{1b} ")?
  test.eq(tui.right_pad("a\u{1b}[3", 4), "a\u{1b}[3   ")?
  test.eq(tui.left_pad("a\u{1b}[31", 4), "   a\u{1b}[31")?
  test.eq(tui.left_pad("\u{1b}[", 2), "  \u{1b}[")?
  test.eq(tui.right_pad("\u{1b}[", 2), "\u{1b}[  ")?
}

proc test_tui_read_secret_piped_lines(ctx: TestContext) [fs, process, error] {
  let script = test.temp_file(
    ctx,
    name: "read-secret.xsh",
    contents: b"let one = tui.read_secret(\"One: \")?\nlet two = tui.read_secret(\"Two: \")?\nprint f\"${one}:${two}\"\n",
  )?

  let input = test.temp_file(ctx, name: "secret.in", contents: b"alpha\nbeta\n")?

  test.eq(
    run.text "xsh" $script < ${input}?,
    """One: Two: alpha:beta
""",
  )?
}
