proc test_cut_fields(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "table.txt", contents: b"a,b,c\n1,2,3\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cut.xsh" -- -d , -f 2 $input ?
  test.contains(output, "b")?
  test.contains(output, "2")?
}

proc test_cut_reads_files_and_stdin_in_operand_order(ctx: TestContext) [fs, process, env, error] {
  let first = test.temp_file(ctx, name: "first.txt", contents: b"a,b\n")?
  let middle = test.temp_file(ctx, name: "middle.txt", contents: b"c,d\n")?
  let last = test.temp_file(ctx, name: "last.txt", contents: b"e,f\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cut.xsh" -- -d , -f 2 $first - $last < ${middle} ?
  test.eq(
    output,
    """b
d
f
""",
  )?
}
