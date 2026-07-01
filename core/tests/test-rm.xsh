pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_rm_force_recursive(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "rm")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  fp"${dir}/nested.txt".write("nested")?
  run.text xsh_bin() rm.xsh -- -rf $dir fp"${root}/missing" ?
  test.ok(! dir.exists()?)?
}
