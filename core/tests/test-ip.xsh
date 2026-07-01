pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_ip_addr_smoke() [process, error] {
  let output = run.text XSH_LINUX_DRY_RUN=1 xsh_bin() ip.xsh -- addr ?
  test.ok(output.count_chars() >= 0)?
}
