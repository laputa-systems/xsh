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

proc test_cp_file_and_recursive_dir(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "cp")?
  let src = fp"${root}/src.txt"
  let dst = fp"${root}/dst.txt"
  src.write("hello")?
  run.text xsh_bin() core_script("cp.xsh") -- $src $dst ?
  test.eq(dst.read_text()?, "hello")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  fp"${dir}/nested.txt".write("nested")?
  let out = fp"${root}/out"
  run.text xsh_bin() core_script("cp.xsh") -- -R $dir $out ?
  test.eq(fp"${out}/nested.txt".read_text()?, "nested")?
}
