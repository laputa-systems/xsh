pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_mv_file_and_target_directory(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "mv")?
  let src = fp"${root}/src.txt"
  src.write("hello")?
  let dir = fp"${root}/dir"
  dir.mkdir()?
  run.text xsh_bin() mv.xsh -- -t $dir $src ?
  test.ok(! src.exists()?)?
  test.eq(fp"${dir}/src.txt".read_text()?, "hello")?
}
