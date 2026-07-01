pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_bench() [process, error] {
  let output = run.text xsh_bin() "showcase/bench.xsh" -- --runs=1 true ?
  test.contains(output, "n=1")?
  test.contains(output, "mean=")?
}
