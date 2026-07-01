pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_ln_symbolic_force(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "ln")?
  let src = fp"${root}/src.txt"
  let dst = fp"${root}/dst.txt"
  src.write("new")?
  dst.write("old")?
  run.text xsh_bin() ln.xsh -- -sf $src $dst ?
  test.contains(dst.readlink()?.display(), "src.txt")?
}
