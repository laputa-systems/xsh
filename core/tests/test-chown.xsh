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

proc test_chown_current_user(ctx: TestContext) [env, fs, process, error] {
  let target = test.temp_file(ctx, name: "owned.txt", contents: b"payload")?
  let current = user.current()?
  let name = current.name
  let output = run.text xsh_bin() core_script("chown.xsh") -- $name $target ?
  test.eq(output, "")?
  test.eq(target.metadata()?.uid, current.uid)?
  let root = test.temp_dir(ctx, name: "owned-tree")?
  let child = fp"${root}/child.txt"
  child.write("payload")?
  let recursive = run.text xsh_bin() core_script("chown.xsh") -- -R $name $root ?
  test.eq(recursive, "")?
  test.eq(child.metadata()?.uid, current.uid)?
}
