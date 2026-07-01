pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_dirname() [process, error] {
  let output = run.text xsh_bin() dirname.xsh -- /tmp/demo/file.txt ?
  test.eq(output.trim(), "/tmp/demo")?
  let many = run.text xsh_bin() dirname.xsh -- /tmp/a/one.txt /tmp/b/two.txt ?
  test.contains(many, "/tmp/a")?
  test.contains(many, "/tmp/b")?
}
