pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_rev_lines_files_and_stdin(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "rev.txt", contents: b"abc\ncaf\xc3\xa9\n")?
  let output = run.text xsh_bin() rev.xsh -- $input ?

  test.eq(
    output,
    """cba
éfac
""",
  )?

  let script = p"rev.xsh"

  let command = f"""printf 'one
two
' | ${xsh_bin().display()} ${script.display()}"""

  let stdin_output = run.text sh -c $command ?

  test.eq(
    stdin_output,
    """eno
owt
""",
  )?
}

proc test_rev_rejects_options(ctx: TestContext) [fs, process, error] {
  let err = test.temp_path(ctx, name: "rev.err")
  let status = run.status xsh_bin() rev.xsh -- -z 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
