proc test_fold_width(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/fold.xsh" -- -w 3 $input ?
  test.contains(output, "abc")?
  test.contains(output, "def")?
}
