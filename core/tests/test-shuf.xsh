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

proc test_shuf_head_count(ctx: TestContext) [env, fs, process, error] {
  let input = test.temp_file(ctx, name: "shuf.txt", contents: b"a\nb\nc\n")?
  let output = run.text xsh_bin() core_script("shuf.xsh") -- -n 2 $input ?
  test.eq(output.count_lines(), 2)?
}
