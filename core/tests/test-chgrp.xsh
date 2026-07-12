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

proc test_chgrp_current_group(ctx: TestContext) [fs, process, env, error] {
  let target = test.temp_file(ctx, name: "grouped.txt", contents: b"payload")?
  let current = group.current()?
  let name = current.name
  let output = run.text xsh_bin() core_script("chgrp.xsh") -- $name $target ?
  test.eq(output, "")?
  test.eq(target.metadata()?.gid, current.gid)?
  let root = test.temp_dir(ctx, name: "grouped-tree")?
  let child = fp"${root}/child.txt"
  child.write("payload")?
  let recursive = run.text xsh_bin() core_script("chgrp.xsh") -- -R $name $root ?
  test.eq(recursive, "")?
  test.eq(child.metadata()?.gid, current.gid)?
}
