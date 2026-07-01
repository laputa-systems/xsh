pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_io_stdin_text_line_bytes_and_stdout(ctx: TestContext) [fs, process, error] {
  test.ok(xsh_bin().exists()?, "child xsh binary should be built before stdlib tests")?

  let text_script = test.temp_file(
    ctx,
    name: "io-text.xsh",
    contents: b"let data = io.stdin_text()?\nio.write_stdout(data)?\n",
  )?

  let text_input = test.temp_file(ctx, name: "text.in", contents: b"hello\nworld\n")?

  test.eq(
    run.text xsh_bin() $text_script < ${text_input}?,
    """hello
world
""",
  )?

  let line_script = test.temp_file(ctx, name: "io-line.xsh", contents: b"let line = io.stdin_line()?\nprint ${line}\n")?
  let line_input = test.temp_file(ctx, name: "line.in", contents: b"first\r\nsecond\n")?

  test.eq(
    run.text xsh_bin() $line_script < ${line_input}?,
    """first
""",
  )?

  let bytes_script = test.temp_file(
    ctx,
    name: "io-bytes.xsh",
    contents: b"let data = io.stdin_bytes()?\nio.write_stdout_bytes(data)?\n",
  )?

  let bytes_input = test.temp_file(ctx, name: "bytes.in", contents: b"\0abc\xff")?
  test.eq(run.bytes xsh_bin() $bytes_script < ${bytes_input}?, b"\0abc\xff")?
}
