proc test_uniq_counts(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "uniq.txt", contents: b"a\na\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/uniq.xsh" -- -c $input ?
  test.contains(output, "2 a")?
  test.contains(output, "1 b")?
}
