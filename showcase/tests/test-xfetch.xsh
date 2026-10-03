test test_xfetch_summary {
  let output = run.text "xsh" "showcase/xfetch.xsh" ?
  "OS" in output
  "Kernel" in output
  "Arch" in output
  "Uptime" in output
  "CPU" in output
  "Memory" in output
  "Root" in output
}
