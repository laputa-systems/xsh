proc test_test_helpers() [error] {
  test.ne(1, 2)?
  test.error_kind(test.fail("covered failure"), "test-fail")?
}

proc test_skip_function_is_covered() {
  test.skip("covered skip")
}
