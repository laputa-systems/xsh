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

proc test_readlink(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "readlink")?
  let target = fp"${root}/target.txt"
  let link = fp"${root}/link.txt"
  target.write("ok")?
  fs.symlink(target, link)?
  let output = run.text xsh_bin() core_script("readlink.xsh") -- $link ?
  test.contains(output, "target.txt")?
  let resolved = run.text xsh_bin() core_script("readlink.xsh") -- -f $link ?
  test.eq(resolved.trim(), target.resolve()?.display())?
  let resolved_long = run.text xsh_bin() core_script("readlink.xsh") -- --canonicalize $link ?
  test.eq(resolved_long.trim(), target.resolve()?.display())?
}
