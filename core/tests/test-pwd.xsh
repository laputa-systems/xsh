pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_pwd() [process, error] {
  let output = run.text xsh_bin() pwd.xsh ?
  test.ok(output.trim().ends_with("/core"))?
}
