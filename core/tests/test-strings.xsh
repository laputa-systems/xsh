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

proc test_strings_min_len(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "strings.bin", contents: b"\0hello\0xy\0there\0")?
  let output = run.text xsh_bin() core_script("strings.xsh") -- -n 5 $input ?
  test.contains(output, "hello")?
  test.contains(output, "there")?
  test.ok(! ("xy" in output))?
}
