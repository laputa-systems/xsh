test test_xfetch_summary {
  let output = run.text "xsh" "showcase/xfetch.xsh" ?
  assert "OS" in output
  assert "Kernel" in output
  assert "Arch" in output
  assert "Uptime" in output
  assert "CPU" in output
  assert "Memory" in output
  assert "Root" in output
}
