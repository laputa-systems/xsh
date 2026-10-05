# Postfix `when`/`unless` and `guard cond else` are sugar for an `if`. Each
# test holds a sugar spelling and the `if` it expands to to the same behavior.

# The first line of a check report that names a diagnostic code, or "" when
# the script was accepted.
proc first_diagnostic(ctx: TestContext, source: Str) [fs, process, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  for line in output.stderr.lines() {
    return line when line.starts_with("err[")
  }

  ""
}

test test_sugar_conditions_report_as_if_conditions { |ctx|
  for source in [
    """pure pick() -> Int { return 1 when 2; return 3 }
""",
    """pure pick() -> Int { return 1 unless 2; return 3 }
""",
    """guard 1 else { exit 1 }
""",
    """if 1 { exit 1 }
""",
  ] {
    let reported = first_diagnostic(ctx, source)?
    assert reported == "err[check.if-condition]: condition must be Bool or Status", source
  }
}

test test_sugar_condition_diagnostic_points_at_the_written_condition { |ctx|
  let output = test.run_script(
    ctx,
    """for n in [1] {
  continue unless n + 1
}
""",
  )?
  assert ! output.success
  assert ":2:19\n" in output.stderr, output.stderr
  assert "^^^^^ found Int" in output.stderr, output.stderr
}

test test_sugar_and_its_if_narrow_the_continuation_alike { |ctx|
  for source in [
    """pure read(raw: Str?) -> Str { return "none" when raw == null; raw.trim() }
print read(" a ")
""",
    """pure read(raw: Str?) -> Str { if raw == null { return "none" }; raw.trim() }
print read(" a ")
""",
    """pure read(raw: Str?) -> Str { return "none" unless raw != null; raw.trim() }
print read(" a ")
""",
    """pure read(raw: Str?) -> Str { if raw != null {} else { return "none" }; raw.trim() }
print read(" a ")
""",
    """pure read(raw: Str?) -> Str { guard raw != null else { return "none" }; raw.trim() }
print read(" a ")
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success, f"{source}{output.stderr}"
    assert output.stdout == "a\n", source
  }

  # A payload that stays in the block proves nothing about what follows.
  for source in [
    """stream rows(raw: Str?) -> Stream[Str] { yield "none" when raw == null; yield raw.trim() }
""",
    """stream rows(raw: Str?) -> Stream[Str] { if raw == null { yield "none" }; yield raw.trim() }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
  }
}

test test_status_conditions_are_not_negated { |ctx|
  let output = test.expect(
    ctx,
    """proc first_failure() [process] -> Str {
  let good = run.status true
  let bad = run.status false
  return "good failed" unless good
  return "bad failed" unless bad
  "none"
}
proc checked() [process] -> Str {
  let bad = run.status false
  guard bad else { return "guarded" }
  "passed"
}
print first_failure()
print checked()
""",
    status: 0,
  )?
  assert output.stdout == "bad failed\nguarded\n"
}

# An `if` none of whose branches ends in a value is a statement even at the
# end of a block, which is what lets a guarded statement end one.
test test_valueless_if_may_end_a_block { |ctx|
  for source in [
    """proc pick(flag: Bool) -> Int {
  let unit = {
    print "block"
    return 1 when flag
  }
  2
}
print pick(true)
print pick(false)
""",
    """proc pick(flag: Bool) -> Int {
  let unit = {
    print "block"
    if flag { return 1 }
  }
  2
}
print pick(true)
print pick(false)
""",
    """proc pick(flag: Bool) -> Int {
  let unit = {
    print "block"
    if flag {} else { return 2 }
  }
  1
}
print pick(true)
print pick(false)
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success, f"{source}{output.stderr}"
    assert output.stdout == "block\n1\nblock\n2\n", source
  }
}

test test_valueless_tail_if_leaves_the_fall_through_rule_in_charge { |ctx|
  for source in [
    """pure pick(flag: Bool) -> Int { return 1 when flag }
""",
    """pure pick(flag: Bool) -> Int { if flag { return 1 } }
""",
    """pure pick(flag: Bool) -> Int { return 1 unless flag }
""",
    """pure pick(flag: Bool) -> Int { guard flag else { return 1 } }
""",
    """pure pick(flag: Bool) -> Int { if flag {} else { return 1 } }
""",
  ] {
    let reported = first_diagnostic(ctx, source)?
    assert reported == "err[check.missing-return]: function can fall through without returning its declared type", source
  }

  # Every path returns, so nothing falls through.
  for source in [
    """pure pick(flag: Bool) -> Int { if flag { return 1 } else { return 2 } }
print pick(false)
""",
    """pure pick() -> Int { guard false else { return 2 } }
print pick()
""",
    """pure pick() -> Int { if false {} else { return 2 } }
print pick()
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.success, f"{source}{output.stderr}"
    assert output.stdout == "2\n", source
  }

  # A branch that ends in a value still makes the `if` a value.
  let reported = first_diagnostic(
    ctx,
    """pure pick(flag: Bool) -> Int { if flag { 1 } }
""",
  )?
  assert reported == "err[check.if-value-else]: value-producing if requires an else branch"
}

test test_guard_failure_block_must_exit_in_statement_and_tail_position { |ctx|
  for source in [
    """proc run_all(flag: Bool) { guard flag else { print "stay" }; print "next" }
""",
    """proc run_all(flag: Bool) { guard flag else { print "stay" } }
""",
    """proc pick(flag: Bool) -> Int { guard flag else { print "stay" } }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert "err[check.guard-fallthrough]" in output.stderr, output.stderr
  }

  # The rule belongs to `guard`; the `if` it expands to may fall through.
  let output = test.expect(
    ctx,
    """proc run_all(flag: Bool) { if flag {} else { print "stay" }; print "next" }
run_all(false)
""",
    status: 0,
  )?
  assert output.stdout == "stay\nnext\n"
}
