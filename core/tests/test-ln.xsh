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

proc test_ln_symbolic_force(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "ln")?
  let src = fp"${root}/src.txt"
  let dst = fp"${root}/dst.txt"
  src.write("new")?
  dst.write("old")?
  run.text xsh_bin() core_script("ln.xsh") -- -sf $src $dst ?
  test.contains(dst.readlink()?.display(), "src.txt")?
}
