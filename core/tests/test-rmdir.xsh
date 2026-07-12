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

proc test_rmdir_parents(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "rmdir")?
  let nested = fp"${root}/a/b/c"
  nested.mkdir()?
  run.text xsh_bin() core_script("rmdir.xsh") -- -p $nested ?
  test.ok(! fp"${root}/a".exists()?)?
}
