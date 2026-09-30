test test_assert_message_runs_only_on_failure [error] { |ctx|
  let result = test.run_script(ctx, r"""proc context() [io] -> Str {
  print "context"
  "extra context"
}
assert true, context()
print "passed"
assert 1 == 2, context()
""")?
  test.ok(! result.success, result.stderr)?
  test.eq(result.stdout, "passed\ncontext\n")?
  test.ok("AssertionError" in result.stderr)?
  test.ok("extra context" in result.stderr)?
  test.ok("1 == 2" in result.stderr)?
}

test test_assert_short_circuit_and_operands_are_evaluated_once [error] { |ctx|
  let result = test.run_script(ctx, r"""proc observed(n: Int) [io] -> Int {
  print f"$n"
  n
}
assert observed(3) < observed(2) < (1 / 0), "chain context"
""")?
  test.ok(! result.success, result.stderr)?
  test.eq(result.stdout, "3\n2\n")?
  test.ok("3 < 2" in result.stderr)?
  test.ok("later operands skipped" in result.stderr)?
  test.ok("chain context" in result.stderr)?
  test.ok(("division-by-zero" not in result.stderr), result.stderr)?

  let conjunction = test.run_script(ctx, "assert 1 == 2 and (1 / 0 == 0), \"conjunction\"\n")?
  test.ok(! conjunction.success, conjunction.stderr)?
  test.ok("1 == 2" in conjunction.stderr)?
  test.ok("right operand skipped" in conjunction.stderr)?
  test.ok(("division-by-zero" not in conjunction.stderr), conjunction.stderr)?

  let disjunction = test.run_script(ctx, "assert 1 == 2 or 3 == 4, \"disjunction\"\n")?
  test.ok(! disjunction.success, disjunction.stderr)?
  test.ok("1 == 2" in disjunction.stderr)?
  test.ok("3 == 4" in disjunction.stderr)?
}

test test_assert_message_propagates_its_own_failure [error] { |ctx|
  let result = test.run_script(ctx, r"""proc condition() [io] -> Bool {
  print "condition"
  false
}
proc message() [error] -> Str { let value = "bad".parse_int()?; f"$value" }
assert condition(), message()
""")?
  test.ok(! result.success, result.stderr)?
  test.eq(result.stdout, "condition\n")?
  test.eq(result.status, 3)?
  test.ok("invalid integer `bad`" in result.stderr)?
  test.ok(("AssertionError" not in result.stderr), result.stderr)?
}

test test_assert_messages_preserve_retry_capture_and_cleanup [error] { |ctx|
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
  test.ok(result.success, result.stderr)?
  test.ok("cleaned\nboolean assertion failed: retry context\n" in result.stdout)?
  test.ok("retry context" in result.stdout)?
}

test test_assert_message_failure_takes_precedence_and_runs_cleanup [error] { |ctx|
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
  test.ok(result.success, result.stderr)?
  test.ok("cleaned\n" in result.stdout)?
  test.ok(("AssertionError" not in result.stdout), result.stdout)?
}

test test_assert_requires_bool_and_str_and_message [error] { |ctx|
  for source in [
    "assert 1, \"message\"\n",
    "let value: Any = true\nassert value, \"message\"\n",
    "assert Ok(true), \"message\"\n",
    "assert true, 1\n",
    "let message: Any = \"message\"\nassert true, message\n",
    "assert true\n",
  ] {
    let invalid = test.run_script(ctx, source)?
    test.ok(! invalid.success, invalid.stderr)?
    test.eq(invalid.stdout, "")?
  }
}

test test_assert_checks_unreached_message_effects_and_error_types [error] { |ctx|
  let effect = test.run_script(ctx, r"""proc message() [io] -> Str { print "message"; "context" }
proc restricted() [] { assert true, message() }
restricted()
""")?
  test.ok(! effect.success, effect.stderr)?
  test.ok("check.effect-violation" in effect.stderr)?

  let propagation = test.run_script(ctx, r"""proc message() [error] -> Str { let value = "1".parse_int()?; f"$value" }
proc restricted() [] { assert true, message() }
restricted()
""")?
  test.ok(! propagation.success, propagation.stderr)?
  test.ok("check.effect-violation" in propagation.stderr)?
}

test test_assert_keyword_labels_and_external_argv_remain_literal [error] { |ctx|
  let result = test.run_script(ctx, "let row = {assert: \"context\"}\nassert row.assert == \"context\", row.assert\nrun printf \"%s\\n\" assert\n")?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "assert\n")?
  let binding = test.run_script(ctx, "let assert = true\n")?
  test.ok(! binding.success, binding.stderr)?
}

test test_assert_bounded_operands_do_not_hide_context [error] { |ctx|
  let result = test.run_script(ctx, "var long = \"x\"\nfor index in range(10000) { long = long + \"x\" }\nassert long == \"y\", \"bounded context\"\n")?
  test.ok(! result.success, result.stderr)?
  test.ok("bounded context" in result.stderr)?
  test.ok(result.stderr.byte_len() < 3000, result.stderr)?
}

test test_assert_nested_operand_failure_does_not_evaluate_outer_message [error] { |ctx|
  let result = test.run_script(ctx, r"""proc operand() -> Int {
  assert false, "inner failure"
  1
}
proc message() [io] -> Str { print "outer message"; "outer context" }
assert 0 < operand() < 3, message()
""")?
  test.ok(! result.success, result.stderr)?
  test.eq(result.stdout, "")?
  test.ok("inner failure" in result.stderr)?
  test.ok(("outer context" not in result.stderr), result.stderr)?
}

test test_assert_context_is_captured_by_nearest_try [error] { |ctx|
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
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "boolean assertion failed: inner context\nouter success\n")?
}

test test_assert_message_propagation_is_captured_by_try [error] { |ctx|
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
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "invalid integer `bad`\n")?
}

test test_assert_cannot_narrow_core_failure_to_a_nominal_error [error] { |ctx|
  let result = test.run_script(ctx, r"""error Narrow = Only(message: Str)
let result: Result[Unit, Narrow] = try {
  assert true, "checked context"
}
print "accepted"
""")?
  test.ok(! result.success, result.stderr)?
  test.ok("check.type-mismatch" in result.stderr)?
}
