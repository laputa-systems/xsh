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

proc test_fold_width(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "wide.txt", contents: b"abcdef\n")?
  let output = run.text xsh_bin() core_script("fold.xsh") -- -w 3 $input ?
  test.contains(output, "abc")?
  test.contains(output, "def")?
}
