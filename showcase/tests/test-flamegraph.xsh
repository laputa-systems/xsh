proc test_flamegraph() [process, error] {
  let output = run.text "xsh" "showcase/flamegraph.xsh" ?
  "<svg" in output
  "Flamegraph" in output
}
