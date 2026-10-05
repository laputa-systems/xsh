test test_perf_collapse {
  let output = run.text "xsh" "showcase/perf-collapse.xsh"
  assert "xsh::runtime::eval::Eval::eval_program" in output
}
