pure runs(count: Int) -> Int {
  var total = 0
  repeat count times {
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

repeat attempts() times {
  print "body"
}
""",
    status: 0,
  )?
  assert output.stdout == "count\nbody\nbody\nbody\n"
}

# A line break in the head leaves an ordinary statement that begins with the
# name `repeat`, which is not a command here.
test test_repeat_head_is_one_line { |ctx|
  let output = test.run_script(
    ctx,
    """repeat [
  1,
  2,
].len() times {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert output.stdout == ""
}

test test_repeat_count_reads_the_value_before_the_loop {
  var remaining = 4
  var total = 0
  repeat remaining times {
    remaining = 0
    total += 1
  }

  assert total == 4
}

test test_repeat_break_and_continue_target_the_repeat {
  var visited = 0
  var counted = 0
  for outer in [1, 2] {
    repeat 5 times {
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
  repeat 3 times {
    repeat cells + 1 times {
      cells += 1
    }
  }

  # 0 -> 1 -> 3 -> 7
  assert cells == 7
}

pure repeat_arm(count: Int) -> Int {
  var total = 0
  match count {
    0 => repeat 2 times { total += 10 }
    else => repeat count times { total += 1 }
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
  repeat times times {
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
repeat 2 + 1 times {
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
    """repeat "x" times {
  print "never"
}
""",
  )?
  assert output.status != 0
  assert "check.type-mismatch" in output.stderr
  assert "expected Int, found Str" in output.stderr
  # The diagnostic points at the count the user wrote.
  assert ":1:8" in output.stderr
  assert output.stdout == ""
}

test test_repeat_body_is_checked_as_a_loop_body { |ctx|
  let output = test.run_script(
    ctx,
    """proc flaky() -> Result[Int] { Ok(1) }
repeat 2 times {
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
repeat 3 times {
  step(index)?
  index += 1
}
""",
  )?
  assert output.status != 0
  assert "step failed" in output.stderr
  assert ":7:" in output.stderr
}

test test_repeat_formats_and_lints_as_written { |ctx|
  let source = """var total = 0
repeat   2+1   times{
  total += 1
}
for _ in range(total) { total += 1 }
print \$total
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "repeat.xsh", contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == """var total = 0

repeat 2 + 1 times {
  total += 1
}

for _ in range(total) { total += 1 }

print \$total
"""

  # The lint reports the written `for`, never the expansion of a `repeat`.
  let linted = run.capture --text --accept=[0, 1] "xsht" lint $candidate ?
  let report = linted.stdout + linted.stderr
  assert report.split("lint.prefer-repeat").len() == 2, report
  let fixed = run.capture --text "xsht" lint --fix $candidate ?
  assert fixed.status.exited_with(0), fixed.stderr
  assert "repeat total times { total += 1 }" in candidate.read_text()?
  assert "range" not in candidate.read_text()?

  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

test test_repeat_expansion_is_invisible_to_grep { |ctx|
  let candidate = test.temp_file(
    ctx,
    name: "repeat-grep.xsh",
    contents: bytes.from_text("""repeat 2 times {
  print "tick"
}
for _ in range(3) {
  print "tock"
}
"""),
  )?
  let found = run.capture --text "xsht" grep "range(N)" $candidate ?
  assert found.stdout.split("range(").len() == 2, found.stdout
  assert ":4:" in found.stdout
}
