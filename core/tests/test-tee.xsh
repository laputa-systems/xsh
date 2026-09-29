proc test_tee_input_file(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "input.txt", contents: b"hello\n")?
  let out = test.temp_path(ctx, name: "out.txt")
  let stdout = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tee.xsh" -- --input $input $out ?

  stdout == """hello
"""

  (out.read_text()?) == """hello
"""
}

proc test_tee_reads_stdin_and_appends(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"second\n")?
  let out = test.temp_path(ctx, name: "append.txt")

  out.write("""first
""")?

  let stdout = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/tee.xsh" -- -a $out < ${input} ?

  stdout == """second
"""

  (out.read_text()?) == """first
second
"""
}

proc test_tee_preserves_non_utf8_bytes_and_appends(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "binary.dat", contents: b"\0\xff\n")?
  let out = test.temp_path(ctx, name: "binary-out.dat")
  (run.bytes ${ctx.xsh_bin} fp"${ctx.core_dir}/tee.xsh" -- --input $input $out?) == b"\0\xff\n"
  (out.read_bytes()?) == b"\0\xff\n"
  (run.bytes ${ctx.xsh_bin} fp"${ctx.core_dir}/tee.xsh" -- -a $out < ${input}?) == b"\0\xff\n"
  (out.read_bytes()?) == b"\0\xff\n\0\xff\n"
}
