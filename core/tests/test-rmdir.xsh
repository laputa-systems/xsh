pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_rmdir_parents(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "rmdir")?
  let nested = fp"${root}/a/b/c"
  nested.mkdir()?
  run.text xsh_bin() rmdir.xsh -- -p $nested ?
  test.ok(! fp"${root}/a".exists()?)?
}
