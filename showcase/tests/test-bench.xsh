proc test_bench() [process, error] {
  let output = run.text "xsh" "showcase/bench.xsh" -- --runs=1 true ?
  test.contains(output, "n=1")?
  test.contains(output, "mean=")?
}
