test test_boolean_guard_evaluates_once_and_refines_success {
  var calls = 0
  guard (if true { calls += 1; true } else { false }) else { return error.fail("unexpected failure") }
  calls == 1
  let name: Str? = "ready"
  guard name != null else { return error.fail("missing name") }
  name.trim() == "ready"
}

test test_boolean_guard_failure_keeps_lexical_loop_target {
  var reached = 0
  for number in [0, 1, 2] {
    guard number != 0 else { continue }
    guard number != 2 else { break }
    reached += number
  }
  reached == 1
}

pure boolean_guard_failure_refinement(name: Str?) -> Str {
  guard name == null else { return name.trim() }
  "missing"
}

pure boolean_guard_pattern_refinement(value: Any) -> Str {
  guard value is Str else { return "unknown" }
  value.trim()
}

test test_boolean_guard_failure_refinement_and_status {
  boolean_guard_failure_refinement(" ready ") == "ready"
  boolean_guard_failure_refinement(null) == "missing"
  boolean_guard_pattern_refinement(" ready ") == "ready"
  boolean_guard_pattern_refinement(7) == "unknown"
  let status = run true
  guard status else { return error.fail("true failed") }
}

test test_boolean_guard_cleanup_precedes_lexical_return { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
proc choose() [error] -> Int {
  defer mark("function cleanup")
  guard false else {
    defer mark("failure cleanup")
    return 7
  }
  0
}

print \${choose()}
""")?
  assert output.success, output.stderr
  output.stdout == "failure cleanup\nfunction cleanup\n7\n"
}

test test_boolean_guard_condition_error_keeps_identity_and_skips_failure { |ctx|
  let output = test.run_script(ctx, """error GuardError = condition(message: Str)
pure rejected() -> Result[Bool] { Err(GuardError.condition(message: "condition failed")) }
guard rejected()? else { abort(7) }
print "unreachable"
""")?
  output.status == 3
  "GuardError.condition" in output.stderr
  "AssertionError.Failed" not in output.stderr
  output.stdout == ""
}

test test_boolean_guard_false_status_uses_author_failure { |ctx|
  let output = test.run_script(ctx, "let status = run false\nguard status else { abort(7) }\nprint \"unreachable\"\n")?
  output.status == 7
  output.stdout == ""
}

test test_boolean_guard_rejects_fallthrough_and_parameters { |ctx|
  for {source, code} in [
    {source: "guard true else { print \"failure\" }\n", code: "check.guard-fallthrough"},
    {source: "guard true else { |failure| abort(1) }\n", code: "check.block-params"},
    {source: "guard 1 else { abort(1) }\n", code: "check.guard-condition"},
    {source: "guard Ok(true) else { abort(1) }\n", code: "check.guard-condition"},
    {source: "guard true else { loop { break } }\n", code: "check.guard-fallthrough"},
    {source: "proc validate(ok: Bool) [] { guard ok else { if ok { return } else { print \"fallthrough\" } } }\n", code: "check.guard-fallthrough"},
    {source: "guard true else { error.fail(\"may fail\")? }\n", code: "check.guard-fallthrough"},
    {source: "proc validate() [] { var name: Str? = \"ready\"; guard name != null else { return }; name = null; let value: Str = name }\n", code: "check.type-mismatch"},
    {source: "var name: Str? = \"ready\"\nproc mutate() [] { name = null }\nproc validate() [] { guard name != null else { return }; mutate(); let value: Str = name }\n", code: "check.type-mismatch"},
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, f"accepted invalid guard: ${source}"
    code in output.stderr
  }
}

pure boolean_guard_validate_jobs(jobs: Int) -> Result[Unit] {
  guard jobs > 0 else {
    return error.fail("jobs must be positive")
  }
}

test test_boolean_guard_failure_branch_owns_the_error {
  boolean_guard_validate_jobs(4)?
  match boolean_guard_validate_jobs(0) {
    Ok(_) => test.fail("non-positive jobs must fail")?
    Err(failure) => {
      !(failure is AssertionError)
      failure.message == "jobs must be positive"
    }
  }
}
