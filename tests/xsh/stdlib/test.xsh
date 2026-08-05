proc test_test_helpers() [error] {
  test.ne(1, 2)?
  test.error_kind(test.fail("covered failure"), "test-fail")?
}

proc test_error_fail_constructs_validation_result() [error] {
  let failure = error.fail("header is missing")
  test.error_kind(failure, "validation")?
}

proc test_run_script_captures_status_env_args_and_bytes(ctx: TestContext) [error] {
  let ok = test.run_script(
    ctx,
    """
print \${ARGV[0]}
print \${env.get("XSH_RUN_SCRIPT_TEST")?}
io.write_stdout_bytes(b"\\xff\\x00a")?
""",
    ["argument"],
    {XSH_RUN_SCRIPT_TEST: "env-value"},
  )?

  test.ok(ok.success, ok.stderr)?
  test.eq(ok.status, 0)?
  test.contains(ok.stdout, "argument", ok.stdout)?
  test.contains(ok.stdout, "env-value", ok.stdout)?
  test.ok(ok.stdout_bytes.ends_with(b"\xff\0a"), ok.stdout)?

  let failed = test.run_script(
    ctx,
    """abort(7)
""",
  )?

  test.ok(! failed.success, failed.stdout)?
  test.eq(failed.status, 7)?
}

proc test_run_xsht_trace_accepts_trace_flags_and_script_args(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(
    ctx,
    """
print \${ARGV[0]}
run true ?
""",
    ["--trace", "--raw"],
    ["script-arg"],
  )?

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """script-arg
""",
  )?

  test.contains(output.stderr, "kind=script.enter")?
  test.contains(output.stderr, "kind=run.start")?
}

proc test_skip_function_is_covered() {
  test.skip("covered skip")
}
