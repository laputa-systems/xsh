pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_split_lines(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "split")?
  let input = fp"${root}/input.txt"

  input.write("""a
b
c
""")?

  let prefix = fp"${root}/chunk-"
  run.text xsh_bin() split.xsh -- -l 2 $input $prefix ?

  test.contains(
    fp"${root}/chunk-aa".read_text()?,
    """a
b""",
  )?

  test.contains(fp"${root}/chunk-ab".read_text()?, "c")?
}
