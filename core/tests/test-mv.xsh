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

proc test_mv_file_and_target_directory(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "mv")?
  let src = fp"${root}/src.txt"
  src.write("hello")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  run.text xsh_bin() core_script("mv.xsh") -- -t $dir $src ?
  test.ok(! src.exists()?)?
  test.eq(fp"${dir}/src.txt".read_text()?, "hello")?
}
