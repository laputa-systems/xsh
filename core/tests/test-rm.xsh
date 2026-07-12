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

proc test_rm_force_recursive(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "rm")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  fp"${dir}/nested.txt".write("nested")?
  run.text xsh_bin() core_script("rm.xsh") -- -rf $dir fp"${root}/missing" ?
  test.ok(! dir.exists()?)?
}
