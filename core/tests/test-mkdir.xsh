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

proc test_mkdir(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "mkdir")?
  let nested = fp"${root}/a/b"
  run.text xsh_bin() core_script("mkdir.xsh") -- -p -m 700 $nested ?
  test.ok(nested.exists()?)?
  test.eq(nested.metadata()?.mode % 512, 448)?
}
