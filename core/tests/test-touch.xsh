pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_touch(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"${root}/created.txt"
  run.text xsh_bin() touch.xsh -- $target ?
  test.ok(target.exists()?)?
  let missing = fp"${root}/missing.txt"
  run.text xsh_bin() touch.xsh -- -c $missing ?
  test.ok(! missing.exists()?)?
}
