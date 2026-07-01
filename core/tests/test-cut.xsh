pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_cut_fields(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "table.txt", contents: b"a,b,c\n1,2,3\n")?
  let output = run.text xsh_bin() cut.xsh -- -d , -f 2 $input ?
  test.contains(output, "b")?
  test.contains(output, "2")?
}
