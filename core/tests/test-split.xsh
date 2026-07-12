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

proc test_split_lines(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "split")?
  let input = fp"${root}/input.txt"

  input.write("""a
b
c
""")?

  let prefix = fp"${root}/chunk-"
  run.text xsh_bin() core_script("split.xsh") -- -l 2 $input $prefix ?

  test.contains(
    fp"${root}/chunk-aa".read_text()?,
    """a
b""",
  )?

  test.contains(fp"${root}/chunk-ab".read_text()?, "c")?
}
