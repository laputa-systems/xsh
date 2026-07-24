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
