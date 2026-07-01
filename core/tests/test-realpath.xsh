pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_realpath(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text xsh_bin() realpath.xsh -- $root ?
  test.eq(output.trim(), root.resolve()?.display())?
}
