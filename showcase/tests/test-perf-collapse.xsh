proc test_perf_collapse() [process, error] {
  let output = run.text "xsh" "showcase/perf-collapse.xsh" ?
  test.contains(output, "xsh::runtime::eval::Eval::eval_program")?
}
