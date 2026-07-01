pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_fd_finds_by_name_extension_and_type(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "fd")?
  fp"${root}/alpha.txt".write("a")?
  fp"${root}/beta.log".write("b")?
  let output = run.text xsh_bin() fd.xsh -- alpha -t f -e txt $root ?
  test.contains(output, "alpha.txt")?
  test.ok(! output.contains("beta.log"))?
}

proc test_fd_hidden_and_glob(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "fd-hidden")?
  fp"${root}/.hidden.txt".write("hidden")?
  fp"${root}/visible.txt".write("visible")?
  let hidden_default = run.text xsh_bin() fd.xsh -- hidden $root ?
  test.eq(hidden_default, "")?
  let hidden = run.text xsh_bin() fd.xsh -- --hidden hidden $root ?
  test.contains(hidden, ".hidden.txt")?
  let globbed = run.text xsh_bin() fd.xsh -- --glob "*.txt" $root ?
  test.contains(globbed, "visible.txt")?
}

proc test_fd_multiple_roots_exclude_depth_and_executable(ctx: TestContext) [fs, process, error] {
  let left = test.temp_dir(ctx, name: "fd-left")?
  let right = test.temp_dir(ctx, name: "fd-right")?
  fp"${left}/keep.sh".write("echo keep")?
  fs.chmod(fp"${left}/keep.sh", 0o755)?
  fp"${left}/skip.log".write("skip")?
  fs.mkdir(fp"${left}/nested")?
  fp"${left}/nested/deep.sh".write("deep")?
  fp"${right}/other.sh".write("other")?
  fs.chmod(fp"${right}/other.sh", 0o755)?
  let output = run.text xsh_bin() fd.xsh -- --glob "*.sh" -t x -E "skip*" -d1 $left $right ?
  test.contains(output, "keep.sh")?
  test.contains(output, "other.sh")?
  test.ok(! output.contains("skip.log"))?
  test.ok(! output.contains("deep.sh"))?
}
