test test_bench {
  let output = run.text "xsh" "showcase/bench.xsh" -- --runs=1 true ?
  assert "n=1" in output
  assert "mean=" in output
}
