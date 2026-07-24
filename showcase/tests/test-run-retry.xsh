proc test_run_retry() [process, error] {
  let ok = run.text "xsh" "showcase/run-retry.xsh" -- true ?
  test.contains(ok, "ok (try 1)")?
  let fail = run.text "xsh" "showcase/run-retry.xsh" -- false ?
  test.contains(fail, "failed after 3")?
}
