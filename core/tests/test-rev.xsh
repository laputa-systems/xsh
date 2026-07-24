proc test_rev_lines_files_and_stdin(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "rev.txt", contents: b"abc\ncaf\xc3\xa9\n")?
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/rev.xsh" -- $input ?

  test.eq(
    output,
    """cba
éfac
""",
  )?

  let script = fp"${ctx.core_dir}/rev.xsh"

  let command = f"""printf 'one
two
' | ${ctx.xsh_bin.display()} ${script.display()}"""

  let stdin_output = run.text sh -c $command ?

  test.eq(
    stdin_output,
    """eno
owt
""",
  )?
}

proc test_rev_rejects_options(ctx: TestContext) [fs, process, env, error] {
  let err = test.temp_path(ctx, name: "rev.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/rev.xsh" -- -z 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
