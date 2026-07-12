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

proc test_tail_lines(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "lines.txt", contents: b"one\ntwo\nthree\n")?
  let output = run.text xsh_bin() core_script("tail.xsh") -- -n 2 $input ?
  test.ok(! ("one" in output))?
  test.contains(output, "two")?
  test.contains(output, "three")?
}
