test test_hosts_ping_usage [process, error] {
  let output = run.text "xsh" "showcase/hosts-ping.xsh" -- --help ?
  "usage:" in output
}
