proc test_mkdir(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "mkdir")?
  let nested = fp"${root}/a/b"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/mkdir.xsh" -- -p -m 700 $nested ?
  test.ok(nested.exists()?)?
  test.eq(nested.metadata()?.mode % 512, 448)?
}
