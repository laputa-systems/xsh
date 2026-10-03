test test_touch { |ctx|
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"${root}/created.txt"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/touch.xsh" -- $target ?
  assert target.exists()?
  let missing = fp"${root}/missing.txt"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/touch.xsh" -- -c $missing ?
  assert ! missing.exists()?
}
