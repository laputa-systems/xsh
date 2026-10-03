test test_bench {
  let output = run.text "xsh" "showcase/bench.xsh" -- --runs=1 true ?
  "n=1" in output
  "mean=" in output
}
