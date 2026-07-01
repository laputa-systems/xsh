pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_uniq_counts(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "uniq.txt", contents: b"a\na\nb\n")?
  let output = run.text xsh_bin() uniq.xsh -- -c $input ?
  test.contains(output, "2 a")?
  test.contains(output, "1 b")?
}
