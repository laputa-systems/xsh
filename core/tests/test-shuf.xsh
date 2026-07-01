pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_shuf_head_count(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "shuf.txt", contents: b"a\nb\nc\n")?
  let output = run.text xsh_bin() shuf.xsh -- -n 2 $input ?
  test.eq(output.count_lines(), 2)?
}
