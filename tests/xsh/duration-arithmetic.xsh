pure duration_intervals(left: Duration, right: Duration) -> Int { let count = left / right; count }
pure duration_order(left: Duration, right: Duration) -> Bool { left < right }
pure duration_default(pause: Duration = 250ms * 2 + 1s) -> Duration { pause }

pure duration_budget(base: Duration, count: Int) -> Duration { base * count + 1s }

test test_duration_arithmetic_units_quantization_and_order [error] {
  test.eq(250ms + 1s, 1250ms)?
  test.eq(2s - 500ms, 1500ms)?
  test.eq(250ms * 3, 750ms)?
  test.eq(3 * 250ms, 750ms)?
  test.eq(5ms / 2, 2ms)?
  test.eq(1ms / 2, 0ms)?
  test.eq(7s / 2s, 3)?
  test.eq(0ms * 0, 0ms)?
  test.eq(18446744073709551615ms * 0, 0ms)?
  var assigned = 1s
  assigned += 500ms
  assigned -= 250ms
  assigned *= 2
  assigned /= 5
  test.eq(assigned, 500ms)?
  test.eq(duration_intervals(7s, 2s), 3)?
  test.eq(duration_order(1s, 2s), true)?
  test.eq(duration_budget(250ms, 3), 1750ms)?
  0ms < 1ms <= 1s < 1m < 1h
}

test test_duration_arithmetic_rejects_invalid_dimensions [error] { |ctx|
  for source in ["let bad = 1ms + 1", "let bad = 1ms * 1.0", "let bad = 1ms % 1ms", "let bad = 1 / 1ms"] {
    let result = test.run_script(ctx, source)?
    test.ok(!result.success, result.stderr)?
  }
}

test test_duration_arithmetic_checked_failures [error] { |ctx|
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
    test.ok(!result.success, result.stderr)?
    test.ok(sample.code in result.stderr)?
    test.ok("1:" in result.stderr)?
  }
}

test test_duration_arithmetic_evaluates_operands_once_left_to_right [error] { |ctx|
  let output = test.run_script(ctx, r"""proc duration(value: Duration) [io] -> Duration { print "duration"; value }
proc count(value: Int) [io] -> Int { print "count"; value }
let scaled = count(3) * duration(250ms)
let quantized = duration(5ms) / count(2)
print f"${scaled} ${quantized}"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "count\nduration\nduration\ncount\n750ms 2ms\n")?
}

test test_duration_arithmetic_timeout_inputs_and_zero_retry [process, net, time, error] {
  let budget = 250ms * 2 + 1s
  let plan = process.command_argv("true", ["true"], timeout: budget)
  test.eq(time.measure(plan)?.status.exited_with(0), true)?
  test.error_kind(net.request({method: "GET", url: "ftp://example.invalid/", timeout: budget}), "net-scheme")?
  let selected = retry [1ms / 2, 0ms * 2] { 7 }?
  test.eq(selected, 7)?
}

test test_duration_arithmetic_constant_default_and_adapter_boundaries [error] {
  test.eq(duration_default(), 1500ms)?
  let maximum = 18446744073709551615ms
  test.eq(time.seconds(9223372036854775807), maximum)?
  test.eq(time.seconds(-1), 0ms)?
  test.eq(time.millis(-1), 0ms)?
  test.eq(time.millis(9223372036854775807), 9223372036854775807ms)?
}

test test_duration_arithmetic_comparison_reports_reached_values [error] { |ctx|
  let failed = test.run_script(ctx, "1ms < 2s < 1s\n")?
  test.ok(!failed.success, failed.stderr)?
  test.ok("AssertionError.Failed" in failed.stderr)?
  test.ok("2s < 1s" in failed.stderr)?
}
