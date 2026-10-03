pure duration_intervals(left: Duration, right: Duration) -> Int { let count = left / right; count }
pure duration_order(left: Duration, right: Duration) -> Bool { left < right }
pure duration_default(pause = 250ms * 2 + 1s) -> Duration { pause }

pure duration_budget(base: Duration, count: Int) -> Duration { base * count + 1s }
pure duration_millis_adapter(value: Int) -> Duration { time.millis(value) }

test test_duration_arithmetic_units_quantization_and_order {
  assert 250ms + 1s == 1250ms
  assert 2s - 500ms == 1500ms
  assert 250ms * 3 == 750ms
  assert 3 * 250ms == 750ms
  assert 5ms / 2 == 2ms
  assert 1ms / 2 == 0ms
  assert 7s / 2s == 3
  assert 0ms * 0 == 0ms
  assert 18446744073709551615ms * 0 == 0ms
  var assigned = 1s
  assigned += 500ms
  assigned -= 250ms
  assigned *= 2
  assigned /= 5
  assert assigned == 500ms
  assert duration_intervals(7s, 2s) == 3
  assert duration_order(1s, 2s)
  assert duration_budget(250ms, 3) == 1750ms
  assert 0ms < 1ms <= 1s < 1m < 1h
}

test test_duration_arithmetic_rejects_invalid_dimensions { |ctx|
  for source in ["let bad = 1ms + 1", "let bad = 1ms * 1.0", "let bad = 1ms % 1ms", "let bad = 1 / 1ms"] {
    let result = test.run_script(ctx, source)?
    assert !result.success, result.stderr
  }
}

test test_duration_arithmetic_checked_failures { |ctx|
  for sample in [
    {source: "let bad = 0ms - 1ms", code: "duration-underflow"},
    {source: "let bad = 18446744073709551615ms + 1ms", code: "duration-overflow"},
    {source: "let bad = 18446744073709551615ms * 2", code: "duration-overflow"},
    {source: "let bad = 1ms * -1", code: "duration-negative-factor"},
    {source: "let bad = 1ms / 0", code: "division-by-zero"},
    {source: "let bad = 1ms / -1", code: "division-by-zero"},
    {source: "let bad = 1ms / 0ms", code: "division-by-zero"},
    {source: "let bad = 18446744073709551615ms / 1ms", code: "integer-overflow"},
  ] {
    let result = test.run_script(ctx, sample.source)?
    assert !result.success, result.stderr
    assert sample.code in result.stderr
    assert "1:" in result.stderr
  }
}

test test_duration_arithmetic_evaluates_operands_once_left_to_right { |ctx|
  let output = test.run_script(ctx, r"""proc duration(value: Duration) [io] -> Duration { print "duration"; value }
proc count(value: Int) [io] -> Int { print "count"; value }
let scaled = count(3) * duration(250ms)
let quantized = duration(5ms) / count(2)
print f"${scaled} ${quantized}"
""")?
  assert output.success, output.stderr
  assert output.stdout == "count\nduration\nduration\ncount\n750ms 2ms\n"
}

test test_duration_arithmetic_timeout_inputs_and_zero_retry {
  let budget = 250ms * 2 + 1s
  let plan = process.command_argv("true", ["true"], timeout: budget)
  assert time.measure(plan)?.status.exited_with(0)
  test.error_kind(net.request({method: "GET", url: "ftp://example.invalid/", timeout: budget}), "net-scheme")?
  let selected = retry [1ms / 2, 0ms * 2] { 7 }?
  assert selected == 7
}

test test_duration_arithmetic_constant_default_and_adapter_boundaries {
  assert duration_default() == 1500ms
  let maximum = 18446744073709551615ms
  assert time.seconds(9223372036854775807) == maximum
  assert time.seconds(-1) == 0ms
  assert time.millis(-1) == 0ms
  assert duration_millis_adapter(9223372036854775807) == 9223372036854775807ms
}

test test_duration_arithmetic_comparison_reports_reached_values { |ctx|
  let failed = test.run_script(ctx, "1ms < 2s < 1s\n")?
  assert !failed.success, failed.stderr
  assert "AssertionError.Failed" in failed.stderr
  assert "2s < 1s" in failed.stderr
}
