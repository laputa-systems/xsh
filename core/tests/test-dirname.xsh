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

proc test_dirname() [process, env, error] {
  let output = run.text xsh_bin() core_script("dirname.xsh") -- /tmp/demo/file.txt ?
  test.eq(output.trim(), "/tmp/demo")?
  let many = run.text xsh_bin() core_script("dirname.xsh") -- /tmp/a/one.txt /tmp/b/two.txt ?
  test.contains(many, "/tmp/a")?
  test.contains(many, "/tmp/b")?
}
