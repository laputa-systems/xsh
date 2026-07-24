proc test_touch(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"${root}/created.txt"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/touch.xsh" -- $target ?
  test.ok(target.exists()?)?
  let missing = fp"${root}/missing.txt"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/touch.xsh" -- -c $missing ?
  test.ok(! missing.exists()?)?
}
