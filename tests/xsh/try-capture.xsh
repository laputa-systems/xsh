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
  test.eq(output.stdout, "cleanup\ncleanup\ncleanup\ncleanup\nfalse 1 3\n")?
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
