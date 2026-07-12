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

proc test_uniq_counts(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "uniq.txt", contents: b"a\na\nb\n")?
  let output = run.text xsh_bin() core_script("uniq.xsh") -- -c $input ?
  test.contains(output, "2 a")?
  test.contains(output, "1 b")?
}
