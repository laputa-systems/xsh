pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_tail_lines(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text xsh_bin() tail.xsh -- -n 2 $input ?
  test.ok(! ("one" in output))?
  test.contains(output, "two")?
  test.contains(output, "three")?
}
