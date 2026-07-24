proc test_realpath(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/realpath.xsh" -- $root ?
  test.eq(output.trim(), root.resolve()?.display())?
}
