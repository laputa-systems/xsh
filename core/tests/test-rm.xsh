test test_rm_force_recursive { |ctx|
  let root = test.temp_dir(ctx, name: "rm")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/nested.txt".write("nested")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -- -rf $dir fp"{root}/missing" ?
  assert ! dir.exists()?
}
