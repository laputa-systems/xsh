pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_wait_for_usage() [process, error] {
  let output = run.text xsh_bin() "showcase/wait-for.xsh" -- --help ?
  test.contains(output, "usage:")?
}
