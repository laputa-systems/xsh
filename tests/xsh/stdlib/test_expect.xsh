test test_expect_returns_the_output_of_a_matching_run { |ctx|
  let output = test.expect(
    ctx,
    r"""
print ${args[0]}
print ${env.get("XSH_EXPECT_TEST")?}
print ${io.stdin_text()?.trim()}
eprint "warned once"
exit 3
""",
    status: 3,
    stderr: ["warned"],
    stdout: ["argument", "env-value"],
    args: ["argument"],
    env: {XSH_EXPECT_TEST: "env-value"},
    stdin: b"from stdin\n",
  )?
  assert output.status == 3
  assert ! output.success
  assert output.stdout == "argument\nenv-value\nfrom stdin\n"
  assert output.stderr == "warned once\n"
}

test test_expect_needs_only_a_status { |ctx|
  let output = test.expect(ctx, "print \"quiet\"", status: 0)?
  assert output.stdout == "quiet\n"
}

test test_expect_reports_a_wrong_status_with_the_whole_output { |ctx|
  let failure = test.expect(
    ctx,
    r"""
print "to stdout"
eprint "to stderr"
exit 4
""",
    status: 0,
  )
  test.error_kind(failure, "AssertionError.Failed")
  let message = match failure {
    Ok(_) => "",
    Err(e) => e.message,
  }
  assert "expected status 0" in message, message
  assert "status: 4" in message, message
  assert "stdout:\nto stdout\n" in message, message
  assert "stderr:\nto stderr\n" in message, message
}

test test_expect_reports_every_missing_fragment { |ctx|
  let failure = test.expect(
    ctx,
    r"""
print "alpha"
eprint "beta"
""",
    status: 0,
    stderr: ["beta", "gamma"],
    stdout: ["alpha", "delta"],
  )
  test.error_kind(failure, "AssertionError.Failed")
  let message = match failure {
    Ok(_) => "",
    Err(e) => e.message,
  }
  assert "stderr does not contain \"gamma\"" in message, message
  assert "stdout does not contain \"delta\"" in message, message
  assert "does not contain \"beta\"" not in message, message
  assert "does not contain \"alpha\"" not in message, message
  assert "expected status" not in message, message
}

test test_expect_discards_its_record_like_any_other_value { |ctx|
  let _ = test.expect(ctx, "exit 2", status: 2, name: "exits.xsh")?
}
