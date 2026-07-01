pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_watch_run_once(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "watch")?
  fp"${root}/input.txt".write("hello")?
  let output = run.text xsh_bin() "showcase/watch-run.xsh" -- --root $root --once true ?
  test.contains(output, "watching ")?
  test.contains(output, "[run 1]")?
}
