test test_formatted_runtime_error_uses_interpolation_span { |ctx|
  let result = test.run_xsh(
    ctx,
    """var denominator: Int = 0
print f"before {8 / denominator} after"
""",
  )?
  assert result.status == 3, result.stderr
  assert "division-by-zero" in result.stderr, result.stderr
  assert ":2:17\n" in result.stderr, result.stderr
}
