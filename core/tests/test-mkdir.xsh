pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_mkdir(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "mkdir")?
  let nested = fp"${root}/a/b"
  run.text xsh_bin() mkdir.xsh -- -p -m 700 $nested ?
  test.ok(nested.exists()?)?
  test.eq(nested.metadata()?.mode % 512, 448)?
}
