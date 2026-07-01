pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_flamegraph() [process, error] {
  let output = run.text xsh_bin() "showcase/flamegraph.xsh" ?
  test.contains(output, "<svg")?
  test.contains(output, "Flamegraph")?
}
