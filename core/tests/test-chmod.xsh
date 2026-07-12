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

proc test_chmod_recursive(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "chmod")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  let child = fp"${dir}/child.txt"
  child.write("payload")?
  run.text xsh_bin() core_script("chmod.xsh") -- -R 700 $dir ?
  test.eq(child.metadata()?.mode % 512, 448)?
  run.text xsh_bin() core_script("chmod.xsh") -- 600 $child ?
  run.text xsh_bin() core_script("chmod.xsh") -- u+x,g+r $child ?
  test.eq(child.metadata()?.mode % 512, 480)?
}
