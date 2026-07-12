proc xsh_bin() [env] -> Path {
  let bin = (env.get("CARGO_BIN_EXE_xsh") ?? "")
  if bin != "" {
    return fp"${bin}"
  }
  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = (env.get("XSH_CORE_DIR") ?? "")
  if dir != "" {
    return fp"${dir}/${name}"
  }
  return ../name
}

proc test_cat_file_and_stdin(ctx: TestContext) [env, fs, process, error] {
  let input = test.temp_file(ctx, name: "input.txt", contents: b"file\n")?
  let output = run.text xsh_bin() core_script("cat.xsh") -- $input ?

  test.eq(
    output,
    """file
""",
  )?

  let stdin = test.temp_file(ctx, name: "stdin.txt", contents: b"stdin\n")?
  let stdin_output = run.text xsh_bin() core_script("cat.xsh") < ${stdin} ?

  test.eq(
    stdin_output,
    """stdin
""",
  )?
}
