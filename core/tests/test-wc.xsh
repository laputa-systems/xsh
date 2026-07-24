proc normalized_counts(output: Str) [error] -> Str {
  return output.words().join(" ")
}

proc test_wc_counts(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "words.txt", contents: b"one two\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lwc $input ?
  test.contains(normalized_counts(output), "2 3 14")?
  test.contains(output, "words.txt")?
}

proc test_wc_reads_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "stdin.txt", contents: b"one two\nthree\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/wc.xsh" -- -lwc < ${input} ?
  test.eq(normalized_counts(output), "2 3 14")?
}
