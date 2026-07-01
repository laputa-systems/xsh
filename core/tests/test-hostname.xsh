pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_hostname_short() [process, error] {
  let output = run.text xsh_bin() hostname.xsh -- -s ?
  test.ok(output.trim() != "")?
}
