proc diagnose(ctx: TestContext, name: Str, source: Str) [fs, process, error] -> Result[Str] {
  let output = test.run_xsh(ctx, source)?
  assert output.success == false, name
  Ok(output.stderr)
}

proc assert_single(stderr: Str, name: Str, code: Str) [error] {
  assert f"err[{code}]" in stderr, f"{name}: {stderr}"
  assert stderr.split("err[").len() == 2, f"{name}: {stderr}"
}

test test_unresolved_parameter_is_not_reported_after_a_body_error { |ctx|
  let cases = [
    {name: "unused parameter", source: "pure convert(n: Int) -> Int { n }\npure relay(value) { convert(\"a\") }\n"},
    {name: "parameter passed to the failing call", source: "pure convert(n: Int, m: Int) -> Int { n }\npure relay(value) { convert(value, \"a\") }\n"},
    {name: "unused parameter of a proc", source: "pure convert(n: Int) -> Int { n }\nproc relay(value) { convert(\"a\") }\n"},
  ]
  for {name, source} in cases {
    assert_single(diagnose(ctx, name, source)?, name, "check.type-mismatch")
  }
}

test test_generic_body_rejection_does_not_hide_an_independent_declaration { |ctx|
  let stderr = diagnose(
    ctx,
    "two declarations",
    "pure convert(n: Int) -> Int { n }\npure broken(value) { convert(\"a\") }\nproc invoke(value, action: Proc) { let observed = action.call(); value }\n",
  )?
  assert "err[check.type-mismatch]" in stderr, stderr
  assert "generic effectful callable requires a known checked effect contract" in stderr, stderr
  assert stderr.split("err[").len() == 3, stderr
}

test test_generic_body_rejection_reports_one_diagnostic { |ctx|
  let stderr = diagnose(
    ctx,
    "unsupported body",
    "proc invoke(value, action: Proc) { let observed = action.call(); value }\n",
  )?
  assert_single(stderr, "unsupported body", "check.local-inference")
  assert "generic effectful callable requires a known checked effect contract" in stderr, stderr
}

test test_failed_statement_callback_application_reports_one_diagnostic { |ctx|
  let stderr = diagnose(
    ctx,
    "statement callback",
    "proc emit(value) { print $value; value }\nproc twice(callback, value) { callback(value); callback(value) }\nlet result = twice(emit, 1)?\n",
  )?
  assert_single(stderr, "statement callback", "check.callable-mismatch")
  assert "expected `Unit`, found `Int`" in stderr, stderr
}

test test_dynamic_argument_for_a_field_reader_reports_one_diagnostic { |ctx|
  let stderr = diagnose(
    ctx,
    "dynamic receiver",
    "pure first_name(value) { value.name }\nlet raw = json.decode(\"{\\\"name\\\": \\\"a\\\"}\")?\nprint $first_name(raw)\n",
  )?
  assert_single(stderr, "dynamic receiver", "check.local-inference")
  assert "row argument remains unresolved" in stderr, stderr
}

test test_unrelated_unresolved_call_is_still_reported_beside_another_error { |ctx|
  let stderr = diagnose(
    ctx,
    "two independent errors",
    "pure convert(n: Int) -> Int { n }\npure keep(value) { value }\nlet wrong: Int = convert(\"a\")\nlet kept = keep([])\n",
  )?
  assert "err[check.type-mismatch]" in stderr, stderr
  assert "type argument remains unresolved" in stderr, stderr
  assert stderr.split("err[").len() == 3, stderr
}

test test_failed_callback_call_does_not_reject_later_valid_calls { |ctx|
  let stderr = diagnose(
    ctx,
    "poisoned application",
    "pure both(callback, first, second) { let a = callback(first); let b = callback(second); a }\npure identity(value) { value }\nlet rejected = both(identity, 1, \"text\")\nlet accepted = both(identity, 1, 2)\nlet other = both(identity, \"a\", \"b\")\n",
  )?
  assert_single(stderr, "poisoned application", "check.callable-mismatch")
  assert ":3:" in stderr, stderr
}
