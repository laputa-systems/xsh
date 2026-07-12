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

proc test_realpath(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text xsh_bin() core_script("realpath.xsh") -- $root ?
  test.eq(output.trim(), root.resolve()?.display())?
}
