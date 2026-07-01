pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_xfetch_summary() [process, error] {
  let output = run.text xsh_bin() "showcase/xfetch.xsh" ?
  test.contains(output, "OS")?
  test.contains(output, "Kernel")?
  test.contains(output, "Arch")?
  test.contains(output, "Uptime")?
  test.contains(output, "CPU")?
  test.contains(output, "Memory")?
  test.contains(output, "Root")?
}
