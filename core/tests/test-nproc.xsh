pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_nproc() [process, error] {
  let output = run.text xsh_bin() nproc.xsh ?
  test.ok(output.trim() != "")?
}
