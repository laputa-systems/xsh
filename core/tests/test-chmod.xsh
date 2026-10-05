test test_chmod_recursive { |ctx|
  let root = test.temp_dir(ctx, name: "chmod")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let child = fp"{dir}/child.txt"
  child.write("payload")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- -R 700 $dir ?
  assert child.metadata()?.mode % 512 == 448
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- 600 $child ?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chmod.xsh" -- u+x,g+r $child ?
  assert child.metadata()?.mode % 512 == 480
}
