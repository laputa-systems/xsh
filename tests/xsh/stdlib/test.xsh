test test_test_helpers {
  assert 1 != 2
  test.error_kind(test.fail("covered failure"), "AssertionError.Failed")?
}

test test_error_fail_constructs_validation_result {
  let failure = error.fail("header is missing")
  test.error_kind(failure, "validation")?
}

test test_run_script_captures_status_env_args_and_bytes { |ctx|
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = ok
    assert assertion_condition, assertion_message
  }
  assert ok.status == 0
  {
    let assertion_condition = "argument" in ok.stdout
    let assertion_message = ok.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "env-value" in ok.stdout
    let assertion_message = ok.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = ok.stdout_bytes.ends_with(b"\xff\0a")
    let assertion_message = ok.stdout
    assert assertion_condition, assertion_message
  }

  let failed = test.run_script(
    ctx,
    """abort(7)
""",
  )?

  {
    let assertion_condition = ! failed.success
    let assertion_message = failed.stdout
    assert assertion_condition, assertion_message
  }
  assert failed.status == 7
}

test test_run_xsht_trace_accepts_trace_flags_and_script_args { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
print \${args[0]}
run true ?
""",
    ["--trace", "--raw"],
    ["script-arg"],
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  assert output.stdout == """script-arg
"""

  assert "kind=script.enter" in output.stderr
  assert "kind=run.start" in output.stderr
}

test test_skip_function_is_covered {
  test.skip("covered skip")
}

test test_native_script_arguments_preserve_a_leading_separator { |ctx|
  let source = r"""print ${args.join(",")}"""
  let script = test.run_script(ctx, source, ["--", "one"])?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = script
    assert assertion_condition, assertion_message
  }
  assert script.stdout == """--,one
"""
  let explicit = test.run_xsh(ctx, source, ["--"], ["--", "one"])?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = explicit
    assert assertion_condition, assertion_message
  }
  assert explicit.stdout == """--,one
"""
  let traced = test.run_xsht_trace(ctx, source, [], ["--", "one"])?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = traced
    assert assertion_condition, assertion_message
  }
  assert traced.stdout == """--,one
"""
}
