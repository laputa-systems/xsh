test test_assert_message_runs_only_on_failure { |ctx|
  let result = test.run_script(ctx, r"""proc context() [io] -> Str {
  print "context"
  "extra context"
}
assert true, context()
print "passed"
assert 1 == 2, context()
""")?
  assert ! result.success, result.stderr
  result.stdout == "passed\ncontext\n"
  "AssertionError" in result.stderr
  "extra context" in result.stderr
  "1 == 2" in result.stderr
}

test test_assert_short_circuit_and_operands_are_evaluated_once { |ctx|
  let result = test.run_script(ctx, r"""proc observed(n: Int) [io] -> Int {
  print f"$n"
  n
}
assert observed(3) < observed(2) < (1 / 0), "chain context"
""")?
  assert ! result.success, result.stderr
  result.stdout == "3\n2\n"
  "3 < 2" in result.stderr
  "later operands skipped" in result.stderr
  "chain context" in result.stderr
  assert "division-by-zero" not in result.stderr, result.stderr

  let conjunction = test.run_script(ctx, "assert 1 == 2 and (1 / 0 == 0), \"conjunction\"\n")?
  assert ! conjunction.success, conjunction.stderr
  "1 == 2" in conjunction.stderr
  "right operand skipped" in conjunction.stderr
  assert "division-by-zero" not in conjunction.stderr, conjunction.stderr

  let disjunction = test.run_script(ctx, "assert 1 == 2 or 3 == 4, \"disjunction\"\n")?
  assert ! disjunction.success, disjunction.stderr
  "1 == 2" in disjunction.stderr
  "3 == 4" in disjunction.stderr
}

test test_assert_message_propagates_its_own_failure { |ctx|
  let result = test.run_script(ctx, r"""proc condition() [io] -> Bool {
  print "condition"
  false
}
proc message() [error] -> Str { let value = "bad".parse_int()?; f"$value" }
assert condition(), message()
""")?
  assert ! result.success, result.stderr
  result.stdout == "condition\n"
  result.status == 3
  "invalid integer `bad`" in result.stderr
  assert "AssertionError" not in result.stderr, result.stderr
}

test test_assert_messages_preserve_retry_capture_and_cleanup { |ctx|
  let result = test.run_script(ctx, r"""proc cleanup() [io] { print "cleaned" }
let result: Result[Unit] = retry [] {
  defer cleanup()
  assert false, "retry context"
}
match result {
  Ok(_) => { print "unexpected" }
  Err(error) => { print $error.message }
}
""")?
  assert result.success, result.stderr
  "cleaned\nassertion failed: false: retry context\n" in result.stdout
  "retry context" in result.stdout
}

test test_assert_message_failure_takes_precedence_and_runs_cleanup { |ctx|
  let result = test.run_script(ctx, r"""proc cleanup() [io] { print "cleaned" }
proc message() [error] -> Str { let value = "bad".parse_int()?; f"$value" }
let result: Result[Unit] = retry [] {
  defer cleanup()
  assert false, message()
}
match result {
  Ok(_) => { print "unexpected" }
  Err(error) => { print $error.message }
}
""")?
  assert result.success, result.stderr
  "cleaned\n" in result.stdout
  assert "AssertionError" not in result.stdout, result.stdout
}

test test_assert_requires_bool_condition_and_str_message { |ctx|
  for source in [
    "assert 1, \"message\"\n",
    "let value: Any = true\nassert value, \"message\"\n",
    "assert Ok(true), \"message\"\n",
    "assert true, 1\n",
    "let message: Any = \"message\"\nassert true, message\n",
    "assert 1\n",
    "assert true,\n",
  ] {
    let invalid = test.run_script(ctx, source)?
    assert ! invalid.success, invalid.stderr
    invalid.stdout == ""
  }
}

test test_assert_checks_unreached_message_effects_and_error_types { |ctx|
  let effect = test.run_script(ctx, r"""proc message() [io] -> Str { print "message"; "context" }
proc restricted() [] { assert true, message() }
restricted()
""")?
  assert ! effect.success, effect.stderr
  "check.effect-violation" in effect.stderr

  let propagation = test.run_script(ctx, r"""proc message() [error] -> Str { let value = "1".parse_int()?; f"$value" }
proc restricted() [] { assert true, message() }
restricted()
""")?
  assert ! propagation.success, propagation.stderr
  "check.effect-violation" in propagation.stderr
}

test test_assert_keyword_labels_and_external_argv_remain_literal { |ctx|
  let result = test.run_script(ctx, "let row = {assert: \"context\"}\nassert row.assert == \"context\", row.assert\nrun printf \"%s\\n\" assert\n")?
  assert result.success, result.stderr
  result.stdout == "assert\n"
  let binding = test.run_script(ctx, "let assert = true\n")?
  assert ! binding.success, binding.stderr
}

test test_assert_bounded_operands_do_not_hide_context { |ctx|
  let result = test.run_script(ctx, "var long = \"x\"\nfor index in range(10000) { long = long + \"x\" }\nassert long == \"y\", \"bounded context\"\n")?
  assert ! result.success, result.stderr
  "bounded context" in result.stderr
  assert result.stderr.byte_len() < 3000, result.stderr
}

test test_assert_nested_operand_failure_does_not_evaluate_outer_message { |ctx|
  let result = test.run_script(ctx, r"""proc operand() -> Int {
  assert false, "inner failure"
  1
}
proc message() [io] -> Str { print "outer message"; "outer context" }
assert 0 < operand() < 3, message()
""")?
  assert ! result.success, result.stderr
  result.stdout == ""
  "inner failure" in result.stderr
  assert "outer context" not in result.stderr, result.stderr
}

test test_assert_context_is_captured_by_nearest_try { |ctx|
  let result = test.run_script(ctx, r"""let result: Result[Str] = try {
  let inner: Result[Unit] = try {
    assert false, "inner context"
  }
  match inner {
    Ok(_) => { print "unexpected" }
    Err(failure) => { print $failure.message }
  }
  "outer success"
}
let value = result?
print $value
""")?
  assert result.success, result.stderr
  result.stdout == "assertion failed: false: inner context\nouter success\n"
}

test test_assert_message_propagation_is_captured_by_try { |ctx|
  let result = test.run_script(ctx, r"""proc message() [error] -> Str {
  let value = "bad".parse_int()?
  f"$value"
}
let result: Result[Unit] = try {
  assert false, message()
}
match result {
  Ok(_) => { print "unexpected" }
  Err(failure) => { print $failure.message }
}
""")?
  assert result.success, result.stderr
  result.stdout == "invalid integer `bad`\n"
}

test test_assert_cannot_narrow_core_failure_to_a_nominal_error { |ctx|
  let result = test.run_script(ctx, r"""error Narrow = Only(message: Str)
let result: Result[Unit, Narrow] = try {
  assert true, "checked context"
}
print "accepted"
""")?
  assert ! result.success, result.stderr
  "check.type-mismatch" in result.stderr
}

test test_assert_message_is_optional { |ctx|
  let result = test.run_script(ctx, "assert true\nassert 1 < 2 < 3\nprint \"passed\"\nassert false\n")?
  assert ! result.success, result.stderr
  result.stdout == "passed\n"
  "AssertionError.Failed" in result.stderr
  "assertion failed: false: evaluated to false" in result.stderr
  ":4:8" in result.stderr
}

test test_assert_reports_bare_statement_detail { |ctx|
  let bare = test.run_script(ctx, "[1, 2] == [1, 3]\n")?
  let plain = test.run_script(ctx, "assert [1, 2] == [1, 3]\n")?
  let messaged = test.run_script(ctx, "assert [1, 2] == [1, 3], \"list context\"\n")?
  for result in [bare, plain, messaged] {
    assert ! result.success, result.stderr
    "AssertionError.Failed" in result.stderr
    "assertion failed: [1, 2] == [1, 3]" in result.stderr
    "\nleft: [1, 2]\nright: [1, 3]" in result.stderr
  }
  "assertion failed: [1, 2] == [1, 3]: list context\nleft: [1, 2]" in messaged.stderr
  ":1:8" in plain.stderr

  let text = test.run_script(ctx, "assert \"a\\nold\\n\" == \"a\\nnew\\n\"\n")?
  assert ! text.success, text.stderr
  "diff:" in text.stderr
  "-old" in text.stderr
  "+new" in text.stderr

  let chain = test.run_script(ctx, "assert 1 < 3 < 2\n")?
  assert ! chain.success, chain.stderr
  "assertion failed: 1 < 3 < 2\nordering comparison failed: 3 < 2" in chain.stderr
}
