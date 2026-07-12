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

proc test_head_lines(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text xsh_bin() core_script("head.xsh") -- -n2 $input ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}

proc test_head_reads_stdin() [fs, process, env, error] {
  let xsh = xsh_bin().resolve()?
  let script = core_script("head.xsh").resolve()?

  let command = f"""printf 'one
two
three
' | ${xsh.display()} ${script.display()} -- -n2"""

  let output = run.text sh -c $command ?
  test.contains(output, "one")?
  test.contains(output, "two")?
  test.ok(! ("three" in output))?
}
