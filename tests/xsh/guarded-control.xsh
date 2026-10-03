pure guarded_cached(cached: Str?) -> Str {
  return cached when cached != null
  "missing"
}

pure guarded_unless(cached: Str?) -> Str {
  return cached unless cached == null
  "missing"
}

test test_guarded_return_narrows_selected_branch_and_falls_through {
  assert guarded_cached("hit") == "hit"
  assert guarded_cached(null) == "missing"
  assert guarded_unless("hit") == "hit"
  assert guarded_unless(null) == "missing"
}

test test_guarded_control_checks_condition_before_lazy_payload { |ctx|
  let output = test.run_script(ctx, r"""
proc condition(selected: Bool) [] -> Bool {
  print "condition"
  return selected
}
proc payload() [] -> Int {
  print "payload"
  return 7
}
proc cleanup() [] {
  print "cleanup"
}
proc pick(selected: Bool) [error] -> Int {
  defer cleanup()
  return payload() when condition(selected)
  print "fallback"
  return 9
}
print ${pick(false)}
print ${pick(true)}
let value = loop {
  break payload() unless condition(true)
  break payload() when condition(true)
}
print $value
""")?
  assert output.success, output.stderr
  assert output.stdout == "condition\nfallback\ncleanup\n9\ncondition\npayload\ncleanup\n7\ncondition\ncondition\npayload\n7\n"
}

test test_guarded_yield_skips_unselected_items { |ctx|
  let output = test.run_script(ctx, r"""
stream guarded_items() [] -> Stream[Int] {
  yield 99 when false
  yield 1 unless false
  yield 99 unless true
  yield 2 when true
}
for item in guarded_items() {
  print $item
}
""")?
  assert output.success, output.stderr
  assert output.stdout == "1\n2\n"
}

test test_guarded_run_payload_keeps_literal_argv_and_status_conditions { |ctx|
  let output = test.run_script(ctx, r"""
proc capture(selected: Bool) [process, error] -> Str {
  return (run.text /usr/bin/printf "selected")? when selected
  return "fallback"
}
proc literal_argv() [process, error] -> Str {
  return run.text /usr/bin/printf "%s\n" when unless ?
}
proc status_condition() [process] -> Int {
  return 99 unless (run.status /usr/bin/true)
  return 1 when (run.status /usr/bin/true)
  return 2
}
print ${capture(false)}
print ${capture(true)}
print ${literal_argv()}
print ${status_condition()}
""")?
  assert output.success, output.stderr
  assert output.stdout == "fallback\nselected\nwhen\nunless\n\n1\n"
}

test test_guarded_payload_propagation_and_result_wrapping { |ctx|
  let output = test.run_script(ctx, r"""
proc cleanup() [] {
  print "cleanup"
}
proc payload() [error] -> Int {
  print "payload"
  return "invalid".parse_int()?
}
proc pick(selected: Bool) [error] -> Result[Int] {
  defer cleanup()
  return payload() when selected
  return 9
}
print ${pick(false)?}
let _ = pick(true)?
""")?
  assert ! output.success, output.stderr
  assert output.stdout == "cleanup\n9\npayload\ncleanup\n"
}

test test_guarded_yield_keeps_cleanup_on_early_consumer_exit { |ctx|
  let output = test.run_script(ctx, r"""
proc cleanup() [] {
  print "cleanup"
}
stream values() [error] -> Stream[Int] {
  defer cleanup()
  yield 0 when false
  yield 1 when true
  yield 2 when true
}
for value in values() {
  print $value
  break
}
print "done"
""")?
  assert output.success, output.stderr
  assert output.stdout == "1\ncleanup\ndone\n"
}

# Top-level code has no callable, loop, or producer to leave, so each
# statement that would leave one is a checker error rather than a runtime one.
test test_top_level_control_flow_is_rejected_during_checking { |ctx|
  let cases = [
    ["let x = 1\nif x == 1 {\n  return\n}\nprint \"after\"\n", "check.return-outside-callable"],
    ["return when true\n", "check.return-outside-callable"],
    ["let x = 1\nif x == 1 {\n  break\n}\n", "check.loop-control"],
    ["let x = 1\nif x == 1 {\n  continue\n}\n", "check.loop-control"],
    ["yield 1\n", "check.yield"],
  ]
  for case in cases {
    let output = test.run_script(ctx, case[0])?
    assert ! output.success, output.stderr
    assert output.status == 2, output.stderr
    assert case[1] in output.stderr, output.stderr
    assert output.stdout == ""
  }
}
