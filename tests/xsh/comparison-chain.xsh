proc test_comparison_chain_adjacent_types_and_order() [error] {
  test.eq(0 <= 1 < 2 <= 2, true)?
  test.eq(4 > 3 >= 3 > 1, true)?
  test.eq(1 < 3 > 2, true)?
  test.eq(0 < 0 < 1, false)?
  test.eq(0.0 <= 1.0 < 2.0, true)?
  let nan = 0.0 / 0.0
  test.eq(0.0 < nan < 1.0, false)?
  test.eq("a" < "b" <= "b", true)?
  test.eq(1 + 1 < 3 * 2 <= 6 and true, true)?
  test.eq(false or 1 < 2 < 3, true)?
  test.eq((1 < 2) == true, true)?
}

proc test_comparison_chain_evaluates_reached_operands_once(ctx: TestContext) [error] {
  let reached = test.run_script(ctx, r"""proc observed(n: Int) [io] -> Int {
  print f"${n}"
  return n
}
let result = observed(1) < observed(2) <= observed(3)
print f"${result}"
""")?
  test.ok(reached.success, reached.stderr)?
  test.eq(reached.stdout, "1\n2\n3\ntrue\n")?

  let skipped = test.run_script(ctx, r"""proc observed(n: Int) [io] -> Int {
  print f"${n}"
  return n
}
let result = observed(3) < observed(2) <= observed(1)
print f"${result}"
""")?
  test.ok(skipped.success, skipped.stderr)?
  test.eq(skipped.stdout, "3\n2\nfalse\n")?
}

proc test_comparison_chain_rejects_invalid_adjacent_types(ctx: TestContext) [error] {
  let invalid = test.run_script(ctx, "let value = 0 < 1 < \"two\"\n")?
  test.ok(! invalid.success, invalid.stderr)?
  test.contains(invalid.stderr, "check.type-mismatch")?
  let numeric = test.run_script(ctx, "let value = 0 < 1.0 < 2.0\n")?
  test.ok(! numeric.success, numeric.stderr)?
  test.contains(numeric.stderr, "check.type-mismatch")?
  let grouped = test.run_script(ctx, "let value = (0 < 1) < 2\n")?
  test.ok(! grouped.success, grouped.stderr)?
  test.contains(grouped.stderr, "comparison requires Int, Float, or Str")?
}

proc test_comparison_chain_requires_grouping_for_mixed_tests(ctx: TestContext) [error] {
  for source in [
    "let value = 0 < 1 < 2 == true\n",
    "let value = (0 + 0) < 1 < 2 == true\n",
    "let value = 0 < 1 in [1, 2]\n",
    "let value = 1 in [1, 2] < 3\n",
  ] {
    let invalid = test.run_script(ctx, source)?
    test.ok(! invalid.success, invalid.stderr)?
    test.contains(invalid.stderr, "parse.mixed-comparison")?
  }
  let explicit = test.run_script(ctx, "let value = (0 < 1 < 2) == true\n")?
  test.ok(explicit.success, explicit.stderr)?
}

proc test_comparison_chain_skips_failing_last_operand(ctx: TestContext) [error] {
  let result = test.run_script(ctx, r"""let result = 2 < 1 < (1 / 0)
print f"${result}"
""")?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "false\n")?
  let reached = test.run_script(ctx, "let result = 0 < 1 < (1 / 0)\n")?
  test.ok(! reached.success, reached.stderr)?
  test.contains(reached.stderr, "division-by-zero")?
}

proc test_comparison_chain_bare_assertion_reports_reached_failed_pair(ctx: TestContext) [error] {
  let failed = test.run_script(ctx, r"""proc observed(n: Int) [io] -> Int {
  print f"${n}"
  return n
}
observed(1) < observed(2) < observed(1) < (1 / 0)
""")?
  test.ok(! failed.success, failed.stderr)?
  test.eq(failed.stdout, "1\n2\n1\n")?
  test.contains(failed.stderr, "assertion-failed")?
  test.contains(failed.stderr, "2 < 1")?
  test.ok(! failed.stderr.contains("division-by-zero"), failed.stderr)?
}
