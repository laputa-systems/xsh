proc test_rmdir_parents(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "rmdir")?
  let nested = fp"${root}/a/b/c"
  nested.mkdir()?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rmdir.xsh" -- -p $nested ?
  test.ok(! fp"${root}/a".exists()?)?
}
