proc test_io_stdin_text_line_bytes_and_stdout(ctx: TestContext) [fs, process, error] {
  let text_script = test.temp_file(
    ctx,
    name: "io-text.xsh",
    contents: b"let data = io.stdin_text()?\nio.write_stdout(data)?\n",
  )?

  let text_input = test.temp_file(ctx, name: "text.in", contents: b"hello\nworld\n")?

  test.eq(
    run.text "xsh" $text_script < ${text_input}?,
    """hello
world
""",
  )?

  let line_script = test.temp_file(ctx, name: "io-line.xsh", contents: b"let line = io.stdin_line()?\nprint ${line}\n")?
  let line_input = test.temp_file(ctx, name: "line.in", contents: b"first\r\nsecond\n")?

  test.eq(
    run.text "xsh" $line_script < ${line_input}?,
    """first
""",
  )?

  let bytes_script = test.temp_file(
    ctx,
    name: "io-bytes.xsh",
    contents: b"let data = io.stdin_bytes()?\nio.write_stdout_bytes(data)?\n",
  )?

  let bytes_input = test.temp_file(ctx, name: "bytes.in", contents: b"\0abc\xff")?
  test.eq(run.bytes "xsh" $bytes_script < ${bytes_input}?, b"\0abc\xff")?
}
