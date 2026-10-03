test test_hosts_ping_usage {
  let output = run.text "xsh" "showcase/hosts-ping.xsh" -- --help ?
  "usage:" in output
}
