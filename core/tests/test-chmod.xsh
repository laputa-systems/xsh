pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_chmod_recursive(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "chmod")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  let child = fp"${dir}/child.txt"
  child.write("payload")?
  run.text xsh_bin() chmod.xsh -- -R 700 $dir ?
  test.eq(child.metadata()?.mode % 512, 448)?
  run.text xsh_bin() chmod.xsh -- 600 $child ?
  run.text xsh_bin() chmod.xsh -- u+x,g+r $child ?
  test.eq(child.metadata()?.mode % 512, 480)?
}
