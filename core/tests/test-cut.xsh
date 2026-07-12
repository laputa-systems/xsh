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

proc test_cut_fields(ctx: TestContext) [fs, process, env, error] {
  let input = test.temp_file(ctx, name: "table.txt", contents: b"a,b,c\n1,2,3\n")?
  let output = run.text xsh_bin() core_script("cut.xsh") -- -d , -f 2 $input ?
  test.contains(output, "b")?
  test.contains(output, "2")?
}
