test test_hyperfine_usage {
  let output = run.text "xsh" "showcase/hyperfine.xsh" --help ?
  assert "usage:" in output
}
