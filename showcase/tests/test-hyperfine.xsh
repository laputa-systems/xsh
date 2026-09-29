proc test_hyperfine_usage() [process, error] {
  let output = run.text "xsh" "showcase/hyperfine.xsh" -- --help ?
  "usage:" in output
}
