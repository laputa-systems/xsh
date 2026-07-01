pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_fold_width(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text xsh_bin() fold.xsh -- -w 3 $input ?
  test.contains(output, "abc")?
  test.contains(output, "def")?
}
