pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_cat_file_and_stdin(ctx: TestContext) [fs, process, error] {
  let input = test.temp_file(ctx, name: "input.txt", contents: b"file\n")?
  let output = run.text xsh_bin() cat.xsh -- $input ?

  test.eq(
    output,
    """file
""",
  )?

  let stdin = test.temp_file(ctx, name: "stdin.txt", contents: b"stdin\n")?
  let stdin_output = run.text xsh_bin() cat.xsh < ${stdin} ?

  test.eq(
    stdin_output,
    """stdin
""",
  )?
}
