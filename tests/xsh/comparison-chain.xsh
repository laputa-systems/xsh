test test_comparison_chain_adjacent_types_and_order {
  assert 0 <= 1 < 2 <= 2
  assert 4 > 3 >= 3 > 1
  assert 1 < 3 > 2
  assert ! (0 < 0 < 1)
  assert 0.0 <= 1.0 < 2.0
  let nan = 0.0 / 0.0
  assert ! (0.0 < nan < 1.0)
  assert "a" < "b" <= "b"
  assert 1 + 1 < 3 * 2 <= 6 and true
  assert false or 1 < 2 < 3
  assert (1 < 2) == true
}

test test_comparison_chain_evaluates_reached_operands_once { |ctx|
  let reached = test.run_script(
    ctx,
    r"""proc observed(n: Int) [io] -> Int {
  print f"${n}"
  return n
}
let result = observed(1) < observed(2) <= observed(3)
print f"${result}"
""",
  )?
  assert reached.success, reached.stderr
  assert reached.stdout == """1
2
3
true
"""

  let skipped = test.run_script(
    ctx,
    r"""proc observed(n: Int) [io] -> Int {
  print f"${n}"
  return n
}
let result = observed(3) < observed(2) <= observed(1)
print f"${result}"
""",
  )?
  assert skipped.success, skipped.stderr
  assert skipped.stdout == """3
2
false
"""
}

test test_comparison_chain_rejects_invalid_adjacent_types { |ctx|
  let invalid = test.run_script(
    ctx,
    """let value = 0 < 1 < "two"
""",
  )?
  assert ! invalid.success, invalid.stderr
  assert "check.type-mismatch" in invalid.stderr
  let numeric = test.run_script(
    ctx,
    """let value = 0 < 1.0 < 2.0
""",
  )?
  assert ! numeric.success, numeric.stderr
  assert "check.type-mismatch" in numeric.stderr
  let grouped = test.run_script(
    ctx,
    """let value = (0 < 1) < 2
""",
  )?
  assert ! grouped.success, grouped.stderr
  assert "comparison requires Int, Float, Str, or Duration" in grouped.stderr
}

test test_comparison_chain_requires_grouping_for_mixed_tests { |ctx|
  for source in [
    """let value = 0 < 1 < 2 == true
""",
    """let value = (0 + 0) < 1 < 2 == true
""",
    """let value = 0 < 1 in [1, 2]
""",
    """let value = 1 in [1, 2] < 3
""",
    """let value = 0 < 1 < 2 is Bool
""",
    """let value: Any = 1
let result = value is Int < true
""",
  ] {
    let invalid = test.run_script(ctx, source)?
    assert ! invalid.success, invalid.stderr
    assert "parse.mixed-comparison" in invalid.stderr
  }

  let explicit = test.run_script(
    ctx,
    """let value = (0 < 1 < 2) == true
""",
  )?
  assert explicit.success, explicit.stderr
}

test test_comparison_chain_skips_failing_last_operand { |ctx|
  let result = test.run_script(
    ctx,
    r"""let result = 2 < 1 < 1 / 0
print f"${result}"
""",
  )?
  assert result.success, result.stderr
  assert result.stdout == """false
"""
  let reached = test.run_script(
    ctx,
    """let result = 0 < 1 < 1 / 0
""",
  )?
  assert ! reached.success, reached.stderr
  assert "division-by-zero" in reached.stderr
}

test test_comparison_chain_assertion_reports_reached_failed_pair { |ctx|
  let failed = test.run_script(
    ctx,
    r"""proc observed(n: Int) [io] -> Int {
  print f"${n}"
  return n
}
assert observed(1) < observed(2) < observed(1) < 1 / 0
""",
  )?
  assert ! failed.success, failed.stderr
  assert failed.stdout == """1
2
1
"""
  assert "AssertionError.Failed" in failed.stderr
  assert "2 < 1" in failed.stderr
  assert "division-by-zero" not in failed.stderr, failed.stderr
}
