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

proc test_touch(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "touch")?
  let target = fp"${root}/created.txt"
  run.text xsh_bin() core_script("touch.xsh") -- $target ?
  test.ok(target.exists()?)?
  let missing = fp"${root}/missing.txt"
  run.text xsh_bin() core_script("touch.xsh") -- -c $missing ?
  test.ok(! missing.exists()?)?
}
