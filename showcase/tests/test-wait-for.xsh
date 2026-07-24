proc test_wait_for_usage() [process, error] {
  let output = run.text "xsh" "showcase/wait-for.xsh" -- --help ?
  test.contains(output, "usage:")?
}
