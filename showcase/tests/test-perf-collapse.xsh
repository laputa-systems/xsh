pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_perf_collapse() [process, error] {
  let output = run.text xsh_bin() "showcase/perf-collapse.xsh" ?
  test.contains(output, "xsh::runtime::eval::Eval::eval_program")?
}
