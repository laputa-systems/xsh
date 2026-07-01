pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_hyperfine_usage() [process, error] {
  let output = run.text xsh_bin() "showcase/hyperfine.xsh" -- --help ?
  test.contains(output, "usage:")?
}
