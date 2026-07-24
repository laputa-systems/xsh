proc test_flamegraph() [process, error] {
  let output = run.text "xsh" "showcase/flamegraph.xsh" ?
  test.contains(output, "<svg")?
  test.contains(output, "Flamegraph")?
}
