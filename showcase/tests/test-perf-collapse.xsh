test test_perf_collapse [process, error] {
  let output = run.text "xsh" "showcase/perf-collapse.xsh" ?
  "xsh::runtime::eval::Eval::eval_program" in output
}
