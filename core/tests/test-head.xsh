test test_head_lines [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/head.xsh" -- -n2 $input ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}

test test_head_reads_stdin [fs, process, env, error] { |ctx|
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/head.xsh" -- -n2 < ${input} ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}
