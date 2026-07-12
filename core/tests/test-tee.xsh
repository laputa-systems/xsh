proc xsh_bin() [env] -> Path {
  let bin = env.get("CARGO_BIN_EXE_xsh") ?? ""

  if bin != "" {
    return fp"${bin}"
  }

  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = env.get("XSH_CORE_DIR") ?? ""

  if dir != "" {
    return fp"${dir}/${name}"
  }

  return ../name
}

proc test_tee_input_file(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "input.txt", contents: b"hello\n")?
  let out = test.temp_path(ctx, name: "out.txt")
  let stdout = run.text xsh_bin() core_script("tee.xsh") -- --input $input $out ?

  test.eq(
    stdout,
    """hello
""",
  )?

  test.eq(
    out.read_text()?,
    """hello
""",
  )?
}

proc test_tee_reads_stdin_and_appends(ctx: TestContext) [fs, process, env, error] {
  let out = test.temp_path(ctx, name: "append.txt")

  out.write("""first
""")?

  let xsh = xsh_bin().resolve()?
  let script = core_script("tee.xsh").resolve()?
  let command = f"printf 'second\\n' | ${xsh.display()} ${script.display()} -- -a ${out.display()}"
  let stdout = run.text sh -c $command ?

  test.eq(
    stdout,
    """second
""",
  )?

  test.eq(
    out.read_text()?,
    """first
second
""",
  )?
}
