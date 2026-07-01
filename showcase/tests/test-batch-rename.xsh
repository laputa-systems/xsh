pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_batch_rename(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "rename")?
  fp"${root}/hello world.txt".write("a")?
  fp"${root}/foo bar.txt".write("b")?
  let dry = run.text xsh_bin() "showcase/batch-rename.xsh" -- --root $root --normalize --dry-run ?
  test.contains(dry, "would rename")?
  test.contains(dry, "hello_world.txt")?
  let actual = run.text xsh_bin() "showcase/batch-rename.xsh" -- --root $root --normalize --dry-run=false ?
  test.contains(actual, "2 files renamed")?
  test.ok(fp"${root}/hello_world.txt".exists()?)?
}
