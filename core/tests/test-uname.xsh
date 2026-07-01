pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_uname_all() [process, error] {
  let output = run.text xsh_bin() uname.xsh -- -a ?
  test.ok(output.fields().len() >= 3)?
}
