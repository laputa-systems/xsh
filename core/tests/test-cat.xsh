proc test_cat_file_and_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "input.txt", contents: b"file\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" -- $input ?

  test.eq(
    output,
    """file
""",
  )?

  let stdin = test.temp_file(ctx, name: "stdin.txt", contents: b"stdin\n")?
  let stdin_output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cat.xsh" < ${stdin} ?

  test.eq(
    stdin_output,
    """stdin
""",
  )?
}
