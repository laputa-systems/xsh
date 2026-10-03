test test_hyperfine_usage {
  let output = run.text "xsh" "showcase/hyperfine.xsh" -- --help ?
  "usage:" in output
}
