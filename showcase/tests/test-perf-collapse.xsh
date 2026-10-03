test test_perf_collapse {
  let output = run.text "xsh" "showcase/perf-collapse.xsh" ?
  "xsh::runtime::eval::Eval::eval_program" in output
}
