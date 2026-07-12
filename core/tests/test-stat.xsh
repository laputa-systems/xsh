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

proc test_stat(ctx: TestContext) [fs, process, env, error] {
  let target = test.temp_file(ctx, name: "stat.txt", contents: b"hello")?
  let output = run.text xsh_bin() core_script("stat.xsh") -- $target ?
  test.contains(output, "kind file")?
  test.contains(output, "size 5")?
  let formatted = run.text xsh_bin() core_script("stat.xsh") -- -c "%s %F %n" $target ?
  test.contains(formatted, "5 regular file")?
  test.contains(formatted, "stat.txt")?
  let modes = run.text xsh_bin() core_script("stat.xsh") -- -c "%a %A %U %G" $target ?
  test.contains(modes, "rw")?
}
