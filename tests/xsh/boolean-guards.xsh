proc test_boolean_guard_evaluates_once_and_refines_success() [error] {
  var calls = 0
  guard (if true { calls += 1; true } else { false }) else { return error.fail("unexpected failure") }
  test.eq(calls, 1)?
  let name: Str? = "ready"
  guard name != null else { return error.fail("missing name") }
  test.eq(name.trim(), "ready")?
}

proc test_boolean_guard_failure_keeps_lexical_loop_target() [error] {
  var reached = 0
  for number in [0, 1, 2] {
    guard number != 0 else { continue }
    guard number != 2 else { break }
    reached += number
  }
  test.eq(reached, 1)?
}

pure boolean_guard_failure_refinement(name: Str?) -> Str {
  guard name == null else { return name.trim() }
  "missing"
}

pure boolean_guard_pattern_refinement(value: Any) -> Str {
  guard value is Str else { return "unknown" }
  value.trim()
}

proc test_boolean_guard_failure_refinement_and_status() [process, error] {
  test.eq(boolean_guard_failure_refinement(" ready "), "ready")?
  test.eq(boolean_guard_failure_refinement(null), "missing")?
  test.eq(boolean_guard_pattern_refinement(" ready "), "ready")?
  test.eq(boolean_guard_pattern_refinement(7), "unknown")?
  let status = run true
  guard status else { return error.fail("true failed") }
}

proc test_boolean_guard_cleanup_precedes_lexical_return(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
proc choose() [] -> Int {
  defer mark("function cleanup")
  guard false else {
    defer mark("failure cleanup")
    return 7
  }
  0
}

print \${choose()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "failure cleanup\nfunction cleanup\n7\n")?
}

proc test_boolean_guard_condition_error_keeps_identity_and_skips_failure(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """error GuardError = condition(message: Str)
pure rejected() -> Result[Bool] { Err(GuardError.condition(message: "condition failed")) }
guard rejected()? else { abort(7) }
print "unreachable"
""")?
  test.eq(output.status, 3)?
  test.contains(output.stderr, "GuardError.condition")?
  test.eq(output.stderr.contains("assertion-failed"), false)?
  test.eq(output.stdout, "")?
}

proc test_boolean_guard_false_status_uses_author_failure(ctx: TestContext) [error] {
  let output = test.run_script(ctx, "let status = run false\nguard status else { abort(7) }\nprint \"unreachable\"\n")?
  test.eq(output.status, 7)?
  test.eq(output.stdout, "")?
}

proc test_boolean_guard_rejects_fallthrough_and_parameters(ctx: TestContext) [error] {
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
    test.ok(! output.success, f"accepted invalid guard: ${source}")?
    test.contains(output.stderr, code)?
  }
}
