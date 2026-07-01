pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_hosts_ping_usage() [process, error] {
  let output = run.text xsh_bin() "showcase/hosts-ping.xsh" -- --help ?
  test.contains(output, "usage:")?
}
