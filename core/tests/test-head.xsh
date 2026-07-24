proc test_head_lines(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/head.xsh" -- -n2 $input ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}

proc test_head_reads_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/head.xsh" -- -n2 < ${input} ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}
