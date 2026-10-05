test test_try_captures_once_and_preserves_result_data { |ctx|
  let output = test.run_script(
    ctx,
    """
var calls = 0
proc operation() -> Result[Int] {
  calls += 1
  Ok(7)
}
let value = try { operation()? }?
let outer = try { operation() }?
let nested = outer?
print f"{value} {nested} {calls}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """7 7 2
"""
}

test test_try_captures_nearest_nominal_error_and_returns_lexically { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
proc fail() [] -> Result[Int, LocalError] { Err(LocalError.Failed(message: "local")) }
proc local() [] -> Result[Int, LocalError] {
  try { fail()? }
}
proc escape() -> Result[Str] {
  let value: Result[Int] = try { return Ok("outer") }
  Ok("after")
}
let inner: Result[Int, LocalError] = try { Err(LocalError.Failed(message: "nearest"))? }
let outer = try { inner }
print (outer? is Err(LocalError.Failed))
print (local() is Err(LocalError.Failed))
print escape()?
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
true
outer
"""
}

test test_try_bool_empty_cleanup_and_loop_targets { |ctx|
  let output = test.run_script(
    ctx,
    """
var cleaned = 0
proc cleanup() -> Result[Unit] { print "cleanup"; cleaned += 1 }
let value = try {
  defer cleanup()?
  false
}?
let empty = try {}?
var rounds = 0
while rounds < 3 {
  rounds += 1
  let captured: Result[Unit] = try {
    defer cleanup()?
    if rounds < 3 { continue }
    break
  }
}
print f"{value} {cleaned} {rounds}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """cleanup
cleanup
cleanup
cleanup
false 4 3
"""
}

test test_try_assertions_capture_but_trace_has_no_retry_events { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
let result = try {
  assert false
  7
}
print (result is Err(_))
""",
    ["--raw", "--trace-format", "jsonl"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
  assert "retry.attempt" not in output.stderr
}

test test_try_outer_propagation_still_requires_error_effect { |ctx|
  let output = test.run_script(
    ctx,
    """
proc bad() [] -> Int {
  try { 7 }?
}
""",
  )?
  assert ! output.success
  assert "check.effect-violation" in output.stderr
}

test test_try_error_only_requires_success_annotation { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
let result = try { Err(LocalError.Failed(message: "unknown success"))? }
""",
  )?
  assert ! output.success
  assert "check.try-success-type" in output.stderr
}

test test_try_unit_context_asserts_and_auto_propagates { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
proc unit_fail() [] -> Result[Unit, LocalError] { Err(LocalError.Failed(message: "unit")) }
let assertion: Result[Unit] = try { assert false }
let failure: Result[Unit, LocalError] = try { unit_fail() }
let success: Result[Unit] = try { assert true }
print (assertion is Err(_))
print (failure is Err(LocalError.Failed))
print (success is Ok(_))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
true
true
"""
}

test test_try_cleanup_failure_is_captured_and_primary_error_wins { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
proc cleanup() [] -> Result[Unit, LocalError] { Err(LocalError.Failed(message: "cleanup")) }
let cleanup_failure: Result[Int, LocalError] = try {
  defer cleanup()?
  7
}
let primary: Result[Int, LocalError] = try {
  defer cleanup()?
  Err(LocalError.Failed(message: "primary"))?
}
match cleanup_failure {
  Err(error) => print \$error.message
  Ok(_) => print "unexpected"
}
match primary {
  Err(error) => print \$error.message
  Ok(_) => print "unexpected"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """cleanup
primary
"""
}

test test_try_rejects_incompatible_nominal_error_annotation { |ctx|
  let output = test.run_script(
    ctx,
    """
error FirstError = Failed(message: Str)
error SecondError = Failed(message: Str)
proc fail() [] -> Result[Int, FirstError] { Err(FirstError.Failed(message: "first")) }
let value: Result[Int, SecondError] = try { fail()? }
""",
  )?
  assert ! output.success
  assert "type mismatch" in output.stderr
}

test test_try_global_assignments_reach_cleanup_and_survive_transfer { |ctx|
  let output = test.run_script(
    ctx,
    """
var count = 0
proc cleanup() -> Result[Unit] {
  print f"cleanup {count}"
  count += 10
}
let value: Result[Int] = try {
  count += 1
  defer cleanup()?
  count
}
print f"{value?} {count}"
var rounds = 0
while rounds < 2 {
  rounds += 1
  defer cleanup()?
  continue
}
print \$count
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """cleanup 1
1 11
cleanup 11
cleanup 21
31
"""
}

test test_try_function_unit_tail_consumes_assertion { |ctx|
  let output = test.run_script(
    ctx,
    """
proc capture() [] -> Result[Unit] { try { assert false } }
print (capture() is Err(_))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
}

test test_try_error_data_needs_its_nested_success_annotation { |ctx|
  let unknown = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
let value = try { Err(LocalError.Failed(message: "nested")) }
""",
  )?
  assert ! unknown.success
  assert "check.try-success-type" in unknown.stderr
  let known = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
let value: Result[Result[Int, LocalError]] = try { Err(LocalError.Failed(message: "nested")) }
print (value? is Err(LocalError.Failed))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = known
    assert assertion_condition, assertion_message
  }
  assert known.stdout == """true
"""
}

test test_try_captures_plain_return_error_effect_call { |ctx|
  let output = test.run_script(
    ctx,
    """
proc fail() [error] -> Int { "invalid".parse_int()? }
proc capture() [] -> Result[Int] { try { fail() } }
print (capture() is Err(_))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
}

test test_try_plain_run_failure_captures_and_abort_escapes { |ctx|
  let failed = test.run_script(
    ctx,
    """
let value: Result[Unit] = try { run false }
print (value is Err(_))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = failed
    assert assertion_condition, assertion_message
  }
  assert failed.stdout == """true
"""
  let aborted = test.expect(
    ctx,
    """
let value: Result[Unit] = try { exit 9 }
print "unexpected"
""",
    status: 9,
  )?
  assert aborted.stdout == ""
}

test test_try_producer_yields_suspend_inside_capture { |ctx|
  let output = test.run_script(
    ctx,
    """
stream rows() -> Stream[Int] {
  let value: Result[Unit] = try {
    yield 1
    yield 2
  }
  value?
}
let values = rows() |> collect()
assert values == [1, 2]
print "yielded"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """yielded
"""
}

test test_try_process_cleanup_preserves_process_error_data { |ctx|
  let output = test.run_script(
    ctx,
    """
proc cleanup() [process, error] -> Result[Unit, ProcessError] { run false }
let value: Result[Int, ProcessError] = try {
  defer cleanup()?
  7
}
print (value is Err(_))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
"""
}

test test_try_producer_capture_and_cancellation_run_cleanup_once { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
proc cleanup() -> Result[Unit, LocalError] { print "cleanup" }
stream rows() -> Stream[Int] {
  let value: Result[Unit, LocalError] = try {
    defer cleanup()?
    yield 1
    Err(LocalError.Failed(message: "after yield"))?
  }
  print (value is Err(LocalError.Failed))
  yield 3
}
let values = rows() |> collect()
assert values == [1, 3]
let first = rows() |> take(1) |> collect()
assert first == [1]
print "finished"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """cleanup
true
cleanup
finished
"""
}

test test_try_recursive_calls_use_frames_and_err_return_stays_lexical { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
proc descend(n: Int) [] -> Result[Int] {
  if n == 0 { Ok(0) } else { try { descend(n - 1)? + 1 } }
}
proc escape() [] -> Result[Str, LocalError] {
  let value: Result[Int, LocalError] = try {
    return Err(LocalError.Failed(message: "outer"))
  }
  Err(LocalError.Failed(message: "unexpected"))
}
print descend(4096)?
match escape() {
  Err(error) => print \$error.message
  Ok(_) => print "unexpected"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """4096
outer
"""
}

test test_try_explicit_return_uses_result_annotation { |ctx|
  let output = test.run_script(
    ctx,
    """
error LocalError = Failed(message: Str)
proc assertion() [] -> Result[Unit] { return try { assert false } }
proc success() [] -> Result[Unit] { return try { assert true } }
proc predicate() [] -> Result[Bool] { return try { false } }
proc nested() [] -> Result[Result[Unit]] { return try { Ok() } }
proc failure() [] -> Result[Int, LocalError] {
  return try { Err(LocalError.Failed(message: "captured"))? }
}
print (assertion() is Err(_))
print (failure() is Err(LocalError.Failed))
print (success() is Ok(_))
print (predicate() is Ok(false))
print (nested() is Ok(Ok(_)))
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """true
true
true
true
true
"""
}
