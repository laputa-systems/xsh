test test_tui_helpers {
  assert tui.clear() == "\u{1b}[2J"
  assert tui.home() == "\u{1b}[H"
  assert tui.erase_line() == "\u{1b}[2K"
  assert tui.hide_cursor() == "\u{1b}[?25l"
  assert tui.show_cursor() == "\u{1b}[?25h"
  assert tui.left_pad("x", 3) == "  x"
  assert tui.right_pad("x", 3) == "x  "
  let styled = f"{tui.red()}x{tui.reset()}"
  assert styled == "\u{1b}[31mx\u{1b}[0m"
  assert tui.left_pad(styled, 3) == f"  {styled}"
  assert tui.right_pad(styled, 3) == f"{styled}  "
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
    assert left.replace(" ", with: "") == "x"
    assert right.replace(" ", with: "") == "x"
  }
}

# Escape sequences occupy no columns, so styled text pads to its displayed
# width and the sequences stay intact around the inserted spaces.
test test_tui_pad_ignores_escape_sequences {
  let styled = f"{tui.red()}x{tui.reset()}"
  assert tui.left_pad(styled, 1) == styled
  assert tui.left_pad(styled, 3) == f"  {styled}"
  assert tui.right_pad(styled, 3) == f"{styled}  "
  assert tui.left_pad(f"{tui.bold()}wide{tui.reset()}", 6) == f"  {tui.bold()}wide{tui.reset()}"

  # A value made only of escape sequences is zero columns wide.
  assert tui.left_pad(tui.reset(), 2) == f"  {tui.reset()}"
  assert tui.right_pad(tui.clear(), 2) == f"{tui.clear()}  "
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

test test_tui_cell_text_width_policy {
  assert tui.cell_width("日本") == 4
  assert tui.cell_width("e\u{301}") == 1
  assert tui.cell_width("\u{301}") == 1
  assert tui.cell_width("·") == 1
  assert tui.cell_width("😀") == 2
  assert tui.cell_width("👩‍🔬") == 2
}

test test_tui_cell_text_clip_and_pad {
  assert tui.cell_clip("A界B", 3) == "A界"
  assert tui.cell_clip("a界", 2) == "a"
  assert tui.cell_clip("界", 1) == ""
  assert tui.cell_clip("👩‍🔬x", 2) == "👩‍🔬"
  assert tui.cell_clip("界", 0) == ""
  assert tui.cell_clip("界", -1) == ""
  assert tui.cell_left_pad("界", 3) == " 界"
  assert tui.cell_right_pad("界", 3) == "界 "
  assert tui.cell_left_pad("界", 2) == "界"
  assert tui.cell_right_pad("界", 2) == "界"
}

test test_tui_cell_slice_keeps_nonzero_offsets_aligned {
  assert tui.cell_slice("a界b", 1, 2) == "界"
  assert tui.cell_slice("a界b", 0, 2) == "a "
  assert tui.cell_slice("a界b", 2, 2) == " b"
  assert tui.cell_slice("👩‍🔬x", 1, 2) == " x"
  assert tui.cell_slice("a界b", -1, 2) == "a "
  assert tui.cell_slice("a界b", 0, -1) == ""
  assert tui.cell_slice("a界b", 5, 2) == ""
}

test test_tui_safe_single_line_makes_hostile_text_inert {
  let hostile = "a\u{1b}[31mred\u{1b}[0m\r\nb\u{1b}]0;title\u{7}c\u{202e}d\u{2028}e"
  let safe = tui.safe_single_line(hostile)
  assert safe == "a [31mred [0m  b ]0;title c�d e"
  assert "\u{1b}" not in safe
  assert "\u{7}" not in safe
  assert "\r" not in safe
  assert "\n" not in safe
  assert "\u{202e}" not in safe
  assert "\u{2028}" not in safe
}

test test_tui_public_apis_compose_a_generic_table_input_and_overlay { |ctx|
  let source = """
use lib.gnu
let query = io.stdin_line()?.trim().lower()
let entries = ["CPU 40.0%", "Memory 75.0%", "Network 10 KiB/s"]
var lines: List[Str] = []
for entry in entries {
  if entry.lower().find(query) != null {
    lines += [tui.cell_right_pad(tui.safe_single_line(entry), 24)]
  }
}
lines += [tui.cell_right_pad("Filter: " + query, 24)]
lines += [tui.cell_right_pad(f"[{lines.len() - 1} rows]  [q] quit", 24)]
gnu.write_text(lines.join("\\n") + "\\n")
"""
  let output = test.run_xsh(ctx, source, [], [], stdin: b"network\n")?
  assert output.success, output.stderr
  let lines = output.stdout.lines()
  assert lines.len() == 3
  assert lines[0].trim() == "Network 10 KiB/s"
  assert lines[1].trim() == "Filter: network"
  assert lines[2].trim() == "[1 rows]  [q] quit"
  for line in lines { assert tui.cell_width(line) == 24 }
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
    contents: b"let one = tui.read_secret(\"One: \")?\nlet two = tui.read_secret(\"Two: \")?\nprint f\"{one}:{two}\"\n",
  )?

  let input = test.temp_file(ctx, name: "secret.in", contents: b"alpha\nbeta\n")?

  assert run.text "xsh" $script < ${input}? == """One: Two: alpha:beta
"""
}
