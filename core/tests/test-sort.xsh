pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_sort_unique_reverse(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "sort.txt", contents: b"b\na\nb\n")?
  let output = run.text xsh_bin() sort.xsh -- -u -r $input ?
  let output_lines = output.lines().collect()
  test.eq(output_lines[0], "b")?
  test.eq(output_lines[1], "a")?
  let keyed = test.temp_file(ctx, name: "keyed.txt", contents: b"b,20\na,3\nc,1\n")?
  let by_second = run.text xsh_bin() sort.xsh -- -t, -k2 -n $keyed ?
  let by_second_lines = by_second.lines().collect()
  test.eq(by_second_lines[0], "c,1")?
  test.eq(by_second_lines[2], "b,20")?
}
