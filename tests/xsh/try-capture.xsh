proc test_try_captures_once_and_preserves_result_data(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
var calls = 0
proc operation() -> Result[Int] {
  calls += 1
  Ok(7)
}
let value = try { operation()? }?
let outer = try { operation() }?
let nested = outer?
print f"\${value} \${nested} \${calls}"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7 7 2\n")?
}

proc test_try_captures_nearest_nominal_error_and_returns_lexically(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\ntrue\nouter\n")?
}

proc test_try_bool_empty_cleanup_and_loop_targets(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
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
print f"\${value} \${cleaned} \${rounds}"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "cleanup\ncleanup\ncleanup\ncleanup\nfalse 4 3\n")?
}

proc test_try_assertions_capture_but_trace_has_no_retry_events(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(ctx, """
let result = try {
  false
  7
}
print (result is Err(_))
""", ["--raw", "--trace-format", "jsonl"])?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\n")?
  test.ok(!output.stderr.contains("retry.attempt"))?
}

proc test_try_outer_propagation_still_requires_error_effect(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
proc bad() [] -> Int {
  try { 7 }?
}
""")?
  test.ok(!output.success)?
  test.contains(output.stderr, "check.effect-violation")?
}

proc test_try_error_only_requires_success_annotation(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error LocalError = Failed(message: Str)
let result = try { Err(LocalError.Failed(message: "unknown success"))? }
""")?
  test.ok(!output.success)?
  test.contains(output.stderr, "check.try-success-type")?
}

proc test_try_unit_context_asserts_and_auto_propagates(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error LocalError = Failed(message: Str)
proc unit_fail() [] -> Result[Unit, LocalError] { Err(LocalError.Failed(message: "unit")) }
let assertion: Result[Unit] = try { false }
let failure: Result[Unit, LocalError] = try { unit_fail() }
let success: Result[Unit] = try { true }
print (assertion is Err(_))
print (failure is Err(LocalError.Failed))
print (success is Ok(_))
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\ntrue\ntrue\n")?
}

proc test_try_cleanup_failure_is_captured_and_primary_error_wins(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "cleanup\nprimary\n")?
}

proc test_try_rejects_incompatible_nominal_error_annotation(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FirstError = Failed(message: Str)
error SecondError = Failed(message: Str)
proc fail() [] -> Result[Int, FirstError] { Err(FirstError.Failed(message: "first")) }
let value: Result[Int, SecondError] = try { fail()? }
""")?
  test.ok(!output.success)?
  test.contains(output.stderr, "type mismatch")?
}

proc test_try_global_assignments_reach_cleanup_and_survive_transfer(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
var count = 0
proc cleanup() -> Result[Unit] {
  print f"cleanup \${count}"
  count += 10
}
let value: Result[Int] = try {
  count += 1
  defer cleanup()?
  count
}
print f"\${value?} \${count}"
var rounds = 0
while rounds < 2 {
  rounds += 1
  defer cleanup()?
  continue
}
print \$count
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "cleanup 1\n1 11\ncleanup 11\ncleanup 21\n31\n")?
}

proc test_try_function_unit_tail_consumes_assertion(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
proc capture() [] -> Result[Unit] { try { false } }
print (capture() is Err(_))
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\n")?
}

proc test_try_error_data_needs_its_nested_success_annotation(ctx: TestContext) [error] {
  let unknown = test.run_script(ctx, """
error LocalError = Failed(message: Str)
let value = try { Err(LocalError.Failed(message: "nested")) }
""")?
  test.ok(!unknown.success)?
  test.contains(unknown.stderr, "check.try-success-type")?
  let known = test.run_script(ctx, """
error LocalError = Failed(message: Str)
let value: Result[Result[Int, LocalError]] = try { Err(LocalError.Failed(message: "nested")) }
print (value? is Err(LocalError.Failed))
""")?
  test.ok(known.success, known.stderr)?
  test.eq(known.stdout, "true\n")?
}

proc test_try_captures_plain_return_error_effect_call(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
proc fail() [error] -> Int { "invalid".parse_int()? }
proc capture() [] -> Result[Int] { try { fail() } }
print (capture() is Err(_))
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\n")?
}

proc test_try_plain_run_failure_captures_and_abort_escapes(ctx: TestContext) [error] {
  let failed = test.run_script(ctx, """
let value: Result[Unit] = try { run false }
print (value is Err(_))
""")?
  test.ok(failed.success, failed.stderr)?
  test.eq(failed.stdout, "true\n")?
  let aborted = test.run_script(ctx, """
let value: Result[Unit] = try { abort(9) }
print "unexpected"
""")?
  test.eq(aborted.status, 9)?
  test.eq(aborted.stdout, "")?
}

proc test_try_producer_yields_suspend_inside_capture(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
stream rows() -> Stream[Int] {
  let value: Result[Unit] = try {
    yield 1
    yield 2
  }
  value?
}
let values = rows() |> collect()
values == [1, 2]
print "yielded"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "yielded\n")?
}
