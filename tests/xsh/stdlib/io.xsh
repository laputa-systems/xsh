test test_io_stdin_text_line_bytes_and_stdout { |ctx|
  let text_script = test.temp_file(
    ctx,
    name: "io-text.xsh",
    contents: b"let data = io.stdin_text()?\nio.write_stdout(data)?\n",
  )?

  let text_input = test.temp_file(ctx, name: "text.in", contents: b"hello\nworld\n")?

  assert run.text "xsh" $text_script < ${text_input}? == """hello
world
"""

  let line_script = test.temp_file(
    ctx,
    name: "io-line.xsh",
    contents: b"let first = io.stdin_line()?\nlet second = io.stdin_line()?\nprint ${first}\nprint ${second}\n",
  )?
  let line_input = test.temp_file(ctx, name: "line.in", contents: b"first\r\nsecond\n")?

  assert run.text "xsh" $line_script < ${line_input}? == """first
second
"""

  let mixed_script = test.temp_file(
    ctx,
    name: "io-line-bytes.xsh",
    contents: b"let first = io.stdin_line()?\nprint ${first}\nio.write_stdout_bytes(io.stdin_bytes()?)?\n",
  )?
  let mixed_input = test.temp_file(ctx, name: "line-bytes.in", contents: b"first\nrest")?
  assert run.bytes "xsh" $mixed_script < ${mixed_input}? == b"first\nrest"

  let bytes_script = test.temp_file(
    ctx,
    name: "io-bytes.xsh",
    contents: b"let data = io.stdin_bytes()?\nio.write_stdout_bytes(data)?\n",
  )?

  let bytes_input = test.temp_file(ctx, name: "bytes.in", contents: b"\0abc\xff")?
  assert run.bytes "xsh" $bytes_script < ${bytes_input}? == b"\0abc\xff"
}

test test_io_write_stderr_preserves_exact_text { |ctx|
  let output = test.run_script(ctx, "io.write_stderr(\"prompt: \")?\n")?
  assert output.success, output.stderr
  assert output.stderr == "prompt: "
}
