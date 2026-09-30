test test_test_helpers [error] {
  1 != 2
  test.error_kind(test.fail("covered failure"), "AssertionError.Failed")?
}

test test_error_fail_constructs_validation_result [error] {
  let failure = error.fail("header is missing")
  test.error_kind(failure, "validation")?
}

test test_run_script_captures_status_env_args_and_bytes [error] { |ctx|
  let ok = test.run_script(
    ctx,
    """
print \${args[0]}
print \${env.get("XSH_RUN_SCRIPT_TEST")?}
io.write_stdout_bytes(b"\\xff\\x00a")?
""",
    ["argument"],
    {XSH_RUN_SCRIPT_TEST: "env-value"},
  )?

  test.ok(ok.success, ok.stderr)?
  ok.status == 0
  test.ok("argument" in ok.stdout, ok.stdout)?
  test.ok("env-value" in ok.stdout, ok.stdout)?
  test.ok(ok.stdout_bytes.ends_with(b"\xff\0a"), ok.stdout)?

  let failed = test.run_script(
    ctx,
    """abort(7)
""",
  )?

  test.ok(! failed.success, failed.stdout)?
  failed.status == 7
}

test test_run_xsht_trace_accepts_trace_flags_and_script_args [error] { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
print \${args[0]}
run true ?
""",
    ["--trace", "--raw"],
    ["script-arg"],
  )?

  test.ok(output.success, output.stderr)?

  output.stdout == """script-arg
"""

  "kind=script.enter" in output.stderr
  "kind=run.start" in output.stderr
}

test test_skip_function_is_covered {
  test.skip("covered skip")
}

test test_native_script_arguments_preserve_a_leading_separator [error] { |ctx|
  let source = r"""print ${args.join(",")}"""
  let script = test.run_script(ctx, source, ["--", "one"])?
  test.ok(script.success, script.stderr)?
  test.eq(script.stdout, "--,one\n")?
  let explicit = test.run_xsh(ctx, source, ["--"], ["--", "one"])?
  test.ok(explicit.success, explicit.stderr)?
  test.eq(explicit.stdout, "--,one\n")?
  let traced = test.run_xsht_trace(ctx, source, [], ["--", "one"])?
  test.ok(traced.success, traced.stderr)?
  test.eq(traced.stdout, "--,one\n")?
}
