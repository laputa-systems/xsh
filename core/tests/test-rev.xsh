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

proc test_rev_lines_files_and_stdin(ctx: TestContext) [env, fs, process, error] {
  let input = test.temp_file(ctx, name: "rev.txt", contents: b"abc\ncaf\xc3\xa9\n")?
  let output = run.text xsh_bin() core_script("rev.xsh") -- $input ?

  test.eq(
    output,
    """cba
éfac
""",
  )?

  let script = core_script("rev.xsh")

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

proc test_rev_rejects_options(ctx: TestContext) [env, fs, process, error] {
  let err = test.temp_path(ctx, name: "rev.err")
  let status = run.status xsh_bin() core_script("rev.xsh") -- -z 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "unsupported option")?
}
