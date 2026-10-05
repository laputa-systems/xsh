test test_boolean_guard_evaluates_once_and_refines_success {
  var calls = 0
  guard if true {
    calls += 1
    true
  } else { false } else {
    return error.fail("unexpected failure")
  }
  assert calls == 1
  let name: Str? = "ready"
  guard name != null else {
    return error.fail("missing name")
  }
  assert name.trim() == "ready"
}

test test_boolean_guard_failure_keeps_lexical_loop_target {
  var reached = 0
  for number in [0, 1, 2] {
    guard number != 0 else {
      continue
    }
    guard number != 2 else {
      break
    }
    reached += number
  }

  assert reached == 1
}

pure boolean_guard_failure_refinement(name: Str?) -> Str {
  guard name == null else {
    return name.trim()
  }
  "missing"
}

pure boolean_guard_pattern_refinement(value: Any) -> Str {
  guard value is Str else {
    return "unknown"
  }
  value.trim()
}

test test_boolean_guard_failure_refinement_and_status {
  assert boolean_guard_failure_refinement(" ready ") == "ready"
  assert boolean_guard_failure_refinement(null) == "missing"
  assert boolean_guard_pattern_refinement(" ready ") == "ready"
  assert boolean_guard_pattern_refinement(7) == "unknown"
  let status = run true
  guard status else {
    return error.fail("true failed")
  }
}

test test_boolean_guard_cleanup_precedes_lexical_return { |ctx|
  let output = test.expect(
    ctx,
    """proc mark(message: Str) [] { print $message }
proc choose() [error] -> Int {
  defer mark("function cleanup")
  guard false else {
    defer mark("failure cleanup")
    return 7
  }
  0
}

print \${choose()}
""",
    status: 0,
  )?
  assert output.stdout == """failure cleanup
function cleanup
7
"""
}

test test_boolean_guard_condition_error_keeps_identity_and_skips_failure { |ctx|
  let output = test.expect(
    ctx,
    """error GuardError = condition(message: Str)
pure rejected() -> Result[Bool] { Err(GuardError.condition(message: "condition failed")) }
guard rejected()? else { exit 7 }
print "unreachable"
""",
    status: 3,
    stderr: ["GuardError.condition"],
  )?
  assert "AssertionError.Failed" not in output.stderr
  assert output.stdout == ""
}

test test_boolean_guard_false_status_uses_author_failure { |ctx|
  let output = test.expect(
    ctx,
    """let status = run false
guard status else { exit 7 }
print "unreachable"
""",
    status: 7,
  )?
  assert output.stdout == ""
}

test test_boolean_guard_rejects_fallthrough_and_parameters { |ctx|
  for {source, code} in [
    {
      source: """guard true else { print "failure" }
""",
      code: "check.guard-fallthrough",
    },
    {
      source: """guard true else { |failure| exit 1 }
""",
      code: "check.block-params",
    },
    {
      source: """guard 1 else { exit 1 }
""",
      code: "check.if-condition",
    },
    {
      # A `Result[Bool]` condition propagates; any other `Result` is no condition.
      source: """guard Ok(1) else { exit 1 }
""",
      code: "check.if-condition",
    },
    {
      source: """guard true else { loop { break } }
""",
      code: "check.guard-fallthrough",
    },
    {
      source: """proc validate(ok: Bool) [] { guard ok else { if ok { return } else { print "fallthrough" } } }
""",
      code: "check.guard-fallthrough",
    },
    {
      source: """guard true else { error.fail("may fail")? }
""",
      code: "check.guard-fallthrough",
    },
    {
      source: """proc validate() [] { var name: Str? = "ready"; guard name != null else { return }; name = null; let value: Str = name }
""",
      code: "check.type-mismatch",
    },
    {
      source: """var name: Str? = "ready"
proc mutate() [] { name = null }
proc validate() [] { guard name != null else { return }; mutate(); let value: Str = name }
""",
      code: "check.type-mismatch",
    },
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, f"accepted invalid guard: {source}"
    assert code in output.stderr
  }
}

pure boolean_guard_validate_jobs(jobs: Int) -> Result[Unit] {
  guard jobs > 0 else {
    return error.fail("jobs must be positive")
  }
}

test test_boolean_guard_failure_branch_owns_the_error {
  boolean_guard_validate_jobs(4)
  match boolean_guard_validate_jobs(0) {
    Ok(_) => test.fail("non-positive jobs must fail")
    Err(failure) => {
      assert ! (failure is AssertionError)
      assert failure.message == "jobs must be positive"
    }
  }
}

# The guard exits the match's scrutinee block on every path, so flow analysis
# types that block `Unknown`; the match is dead code and must not be reported
# as non-exhaustive against that placeholder type.
test test_boolean_guard_exiting_scrutinee_match_is_not_demanded_exhaustive { |ctx|
  let output = test.expect(
    ctx,
    """error FzErr = Bad(message: Str)
pure classify() -> Result[Int, FzErr] {
  match ctx "m" {
    guard false else { return Err(FzErr.Bad(message: "stopped")) }
    [1, 2]
  } {
    [head, ..rest] => head
  }
}
match classify() {
  Ok(value) => print \${value}
  Err(failure) => print \${failure.message}
}
""",
    status: 0,
  )?
  assert output.stdout == """stopped
"""
}
