proc test_split_lines(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "split")?
  let input = fp"${root}/input.txt"

  input.write("""a
b
c
""")?

  let prefix = fp"${root}/chunk-"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/split.xsh" -- -l 2 $input $prefix ?

  test.contains(
    fp"${root}/chunk-aa".read_text()?,
    """a
b""",
  )?

  test.contains(fp"${root}/chunk-ab".read_text()?, "c")?
}
