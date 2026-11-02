pure runs(count: Int) -> Int {
  var total = 0
  for _ in range(count) {
    total += 1
  }

  total
}

test test_repeat_runs_the_block_count_times {
  assert runs(0) == 0
  assert runs(1) == 1
  assert runs(5) == 5
}

# `range` counts down from zero for a negative argument, and `repeat` is that loop.
test test_repeat_negative_count_follows_range {
  let expected = [index for index in range(-3)].len()
  assert runs(-3) == expected
  assert expected == 3
}

test test_repeat_evaluates_the_count_once { |ctx|
  let output = test.expect(
    ctx,
    """proc attempts() -> Int {
  print "count"
  3
}

for _ in range(attempts()) {
  print "body"
}
""",
    status: 0,
  )?
  assert output.stdout == "count\nbody\nbody\nbody\n"
}

test test_repeat_count_reads_the_value_before_the_loop {
  var remaining = 4
  var total = 0
  for _ in range(remaining) {
    remaining = 0
    total += 1
  }

  assert total == 4
}

test test_repeat_break_and_continue_target_the_repeat {
  var visited = 0
  var counted = 0
  for outer in [1, 2] {
    for _ in range(5) {
      visited += 1
      continue when visited % 2 == 0
      break when visited > 6
      counted += outer
    }
  }

  # First pass visits 1..5 and counts 1, 3, 5; the second stops at 7.
  assert visited == 7
  assert counted == 3
}

test test_repeat_nests {
  var cells = 0
  for _ in range(3) {
    for _ in range(cells + 1) {
      cells += 1
    }
  }

  # 0 -> 1 -> 3 -> 7
  assert cells == 7
}

pure repeat_arm(count: Int) -> Int {
  var total = 0
  match count {
    0 => for _ in range(2) { total += 10 }
    else => for _ in range(count) { total += 1 }
  }

  total
}

test test_repeat_is_a_match_arm_statement {
  assert repeat_arm(0) == 20
  assert repeat_arm(3) == 3
}

test test_repeat_and_times_stay_ordinary_names {
  let times = 2
  let repeat = 3
  var total = 0
  for _ in range(times) {
    total += repeat
  }

  assert total == 6
  let copies = [1, 2] |> repeat(count: times) |> collect()
  assert copies == [1, 2, 1, 2]
}

test test_repeat_at_script_top_level { |ctx|
  let output = test.expect(
    ctx,
    """var total = 0
for _ in range(2 + 1) {
  total += 2
}
print \$total
""",
    status: 0,
  )?
  assert output.stdout == "6\n"
}

test test_repeat_count_must_be_an_int { |ctx|
  let output = test.run_script(
    ctx,
    """for _ in range("x") {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert "check.type-mismatch" in output.stderr
  assert "expected Int, found Str" in output.stderr
  # The diagnostic points at the count the user wrote.
  assert ":1:16" in output.stderr
  assert output.stdout == ""
}

test test_repeat_body_is_checked_as_a_loop_body { |ctx|
  let output = test.run_script(
    ctx,
    """proc flaky() -> Result[Int] { Ok(1) }
for _ in range(2) {
  1 == 1
  flaky()
}
""",
  )?
  assert output.status != 0
  assert "check.bool-statement" in output.stderr
  assert ":3:3" in output.stderr
  assert "check.ignored-result" in output.stderr
  assert ":4:3" in output.stderr
}

test test_repeat_needs_times_before_the_block { |ctx|
  let output = test.run_script(
    ctx,
    """let attempts = 2
repeat attempts {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
}

test test_repeat_failure_traceback_names_the_written_lines { |ctx|
  let output = test.run_script(
    ctx,
    """proc step(index: Int) [error] {
  return error.fail("step failed") when index == 1
}

var index = 0
for _ in range(3) {
  step(index)?
  index += 1
}
""",
  )?
  assert output.status != 0
  assert "step failed" in output.stderr
  assert ":7:" in output.stderr
}

test test_range_loop_formats_as_written { |ctx|
  let source = """var total = 0
for _ in range(2+1) {
  total += 1
}
for _ in range(total) { total += 1 }
print \$total
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "repeat.xsh", contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """var total = 0

for _ in range(2 + 1) {
  total += 1
}

for _ in range(total) { total += 1 }

print \$total
"""

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

test test_repeat_statement_is_rejected_with_the_range_loop_to_write { |ctx|
  let output = test.run_script(
    ctx,
    """var total = 0
repeat 2 times {
  total += 1
}
  repeat (total + 1) times { total += 1 }
print \$total
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
  assert output.stderr.split("err[parse.repeat-statement]").len() == 3, output.stderr
  assert "write `for _ in range(2) {`" in output.stderr, output.stderr
  assert "write `for _ in range(total + 1) { total += 1 }`" in output.stderr, output.stderr
  assert "check." not in output.stderr, output.stderr
}
