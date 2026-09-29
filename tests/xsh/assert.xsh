proc test_assert_message_runs_only_on_failure(ctx: TestContext) [error] {
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
  test.contains(result.stderr, "assertion-failed")?
  test.contains(result.stderr, "extra context")?
  test.contains(result.stderr, "1 == 2")?
}

proc test_assert_short_circuit_and_operands_are_evaluated_once(ctx: TestContext) [error] {
  let result = test.run_script(ctx, r"""proc observed(n: Int) [io] -> Int {
  print f"$n"
  n
}
assert observed(3) < observed(2) < (1 / 0), "chain context"
""")?
  test.ok(! result.success, result.stderr)?
  test.eq(result.stdout, "3\n2\n")?
  test.contains(result.stderr, "3 < 2")?
  test.contains(result.stderr, "later operands skipped")?
  test.contains(result.stderr, "chain context")?
  test.ok(! result.stderr.contains("division-by-zero"), result.stderr)?

  let conjunction = test.run_script(ctx, "assert 1 == 2 and (1 / 0 == 0), \"conjunction\"\n")?
  test.ok(! conjunction.success, conjunction.stderr)?
  test.contains(conjunction.stderr, "1 == 2")?
  test.contains(conjunction.stderr, "right operand skipped")?
  test.ok(! conjunction.stderr.contains("division-by-zero"), conjunction.stderr)?

  let disjunction = test.run_script(ctx, "assert 1 == 2 or 3 == 4, \"disjunction\"\n")?
  test.ok(! disjunction.success, disjunction.stderr)?
  test.contains(disjunction.stderr, "1 == 2")?
  test.contains(disjunction.stderr, "3 == 4")?
}

proc test_assert_message_propagates_its_own_failure(ctx: TestContext) [error] {
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
  test.contains(result.stderr, "invalid integer `bad`")?
  test.ok(! result.stderr.contains("assertion-failed"), result.stderr)?
}

proc test_assert_messages_preserve_retry_capture_and_cleanup(ctx: TestContext) [error] {
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
  test.contains(result.stdout, "cleaned\nboolean assertion failed: retry context\n")?
  test.contains(result.stdout, "retry context")?
}

proc test_assert_message_failure_takes_precedence_and_runs_cleanup(ctx: TestContext) [error] {
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
  test.contains(result.stdout, "cleaned\n")?
  test.ok(! result.stdout.contains("assertion-failed"), result.stdout)?
}

proc test_assert_requires_bool_and_str_and_message(ctx: TestContext) [error] {
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

proc test_assert_checks_unreached_message_effects_and_error_types(ctx: TestContext) [error] {
  let effect = test.run_script(ctx, r"""proc message() [io] -> Str { print "message"; "context" }
proc restricted() [] { assert true, message() }
restricted()
""")?
  test.ok(! effect.success, effect.stderr)?
  test.contains(effect.stderr, "check.effect-violation")?

  let propagation = test.run_script(ctx, r"""proc message() [error] -> Str { let value = "1".parse_int()?; f"$value" }
proc restricted() [] { assert true, message() }
restricted()
""")?
  test.ok(! propagation.success, propagation.stderr)?
  test.contains(propagation.stderr, "check.effect-violation")?
}

proc test_assert_keyword_labels_and_external_argv_remain_literal(ctx: TestContext) [error] {
  let result = test.run_script(ctx, "let row = {assert: \"context\"}\nassert row.assert == \"context\", row.assert\nrun printf \"%s\\n\" assert\n")?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "assert\n")?
  let binding = test.run_script(ctx, "let assert = true\n")?
  test.ok(! binding.success, binding.stderr)?
}

proc test_assert_bounded_operands_do_not_hide_context(ctx: TestContext) [error] {
  let result = test.run_script(ctx, "var long = \"x\"\nfor index in range(10000) { long = long + \"x\" }\nassert long == \"y\", \"bounded context\"\n")?
  test.ok(! result.success, result.stderr)?
  test.contains(result.stderr, "bounded context")?
  test.ok(result.stderr.byte_len() < 3000, result.stderr)?
}

proc test_assert_nested_operand_failure_does_not_evaluate_outer_message(ctx: TestContext) [error] {
  let result = test.run_script(ctx, r"""proc operand() -> Int {
  assert false, "inner failure"
  1
}
proc message() [io] -> Str { print "outer message"; "outer context" }
assert 0 < operand() < 3, message()
""")?
  test.ok(! result.success, result.stderr)?
  test.eq(result.stdout, "")?
  test.contains(result.stderr, "inner failure")?
  test.ok(! result.stderr.contains("outer context"), result.stderr)?
}
