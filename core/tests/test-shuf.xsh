proc test_shuf_head_count(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "shuf.txt", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/shuf.xsh" -- -n 2 $input ?
  test.eq(output.count_lines(), 2)?
}
