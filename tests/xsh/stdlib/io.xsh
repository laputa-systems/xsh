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

  let line_script = test.temp_file(ctx, name: "io-line.xsh", contents: b"let line = io.stdin_line()?\nprint ${line}\n")?
  let line_input = test.temp_file(ctx, name: "line.in", contents: b"first\r\nsecond\n")?

  assert run.text "xsh" $line_script < ${line_input}? == """first
"""

  let bytes_script = test.temp_file(
    ctx,
    name: "io-bytes.xsh",
    contents: b"let data = io.stdin_bytes()?\nio.write_stdout_bytes(data)?\n",
  )?

  let bytes_input = test.temp_file(ctx, name: "bytes.in", contents: b"\0abc\xff")?
  assert run.bytes "xsh" $bytes_script < ${bytes_input}? == b"\0abc\xff"
}

test test_io_stdin_lines_preserve_unread_input { |ctx|
  let output = test.run_script(ctx, """
print io.stdin_line()?
print io.stdin_line()?
io.write_stdout_bytes(io.stdin_bytes()?)?
""", stdin: b"first\r\nsecond\ntail")?
  assert output.success, output.stderr
  assert output.stdout == "first\nsecond\ntail"
}

test test_io_stdin_read_bounds_bytes_and_eof { |ctx|
  let output = test.run_script(ctx, """
assert io.stdin_read(2)? == b"\\0\\xff"
assert io.stdin_read(max_bytes: 1)? == b"a"
assert io.stdin_read(8)? == b"bc"
assert io.stdin_read(1)? == b""
assert io.stdin_read(8)? == b""
print "read all bytes"
""", stdin: b"\0\xffabc")?
  assert output.success, output.stderr
  assert output.stdout == "read all bytes\n"
}

test test_io_stdin_read_rejects_invalid_counts_without_consuming_input { |ctx|
  let output = test.run_script(ctx, """
for count in [0, -1, 9223372036854775807] {
  match io.stdin_read(count) {
    Err(_) => {}
    Ok(_) => { assert false, "invalid count succeeded" }
  }
}
assert io.stdin_read(1)? == b"a"
assert io.stdin_text()? == "bc"
""", stdin: b"abc")?
  assert output.success, output.stderr
}

test test_io_stdin_read_line_and_text_share_one_cursor { |ctx|
  let output = test.run_script(ctx, """
assert io.stdin_read(2)? == b"ab"
assert io.stdin_line()? == "c"
assert io.stdin_read(1)? == b"d"
assert io.stdin_text()? == "ef"
assert io.stdin_line()? == ""
""", stdin: b"abc\r\ndef")?
  assert output.success, output.stderr
}

test test_io_stdin_line_invalid_utf8_preserves_following_line { |ctx|
  let output = test.run_script(ctx, """
match io.stdin_line() {
  Err(_) => {}
  Ok(_) => { assert false, "invalid UTF-8 accepted" }
}
assert io.stdin_line()? == "next"
""", stdin: b"\xff\nnext\n")?
  assert output.success, output.stderr
}

test test_io_write_stderr_without_newline_and_flush { |ctx|
  let output = test.run_script(ctx, """
io.write_stderr("Continue? ")?
io.flush_stderr()?
assert io.stdin_line()? == "yes"
io.write_stderr("done")?
print "accepted"
""", stdin: b"yes\n")?
  assert output.success, output.stderr
  assert output.stdout == "accepted\n"
  assert output.stderr == "Continue? done"
}

test test_io_flush_stderr_preserves_captured_output {
  io.write_stderr("captured")?
  io.flush_stderr()?
}

test test_io_flush_stderr_reports_unwritable_descriptor { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{root}/unwritable-stderr.xsh"
  script.write("""
io.write_stderr("prompt")?
match io.flush_stderr() {
  Err(failure) => { assert failure.errno == 9 }
  Ok(_) => { assert false, "unwritable stderr accepted" }
}
print "reported EBADF"
""")
  let output = test.run_script(ctx, r"""
let xsh = applet.current_exe()?
let script = e"SCRIPT"?
let body = "exec 2</dev/null; exec \"$0\" \"$1\""
let text = run.text sh -c $body $xsh $script ?
io.write_stdout(text)?
""", env: {SCRIPT: script})?
  assert output.success, output.stderr
  assert output.stdout == "reported EBADF\n"
}

test test_io_stdin_read_preserves_input_for_inherited_child { |ctx|
  let output = test.run_script(ctx, """
assert io.stdin_read(1)? == b"a"
let remaining = run.bytes cat ?
assert remaining == b"bc"
""", stdin: b"abc")?
  assert output.success, output.stderr
}
