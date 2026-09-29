test test_retry_repeats_until_attempt_succeeds [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  if attempts < 3 {
    return Err(RetryError.Transient(message: f"attempt \${attempts}"))
  }
  return Ok("done")
}

let value = retry [1ms / 2, 0ms * 2] {
  flaky()?
}?
print f"\${value} \${attempts}"
""",
  )?

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """done 3
""",
  )?

  test.eq(output.stderr, "")?
}

test test_retry_exhaustion_returns_final_error [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  return Err(RetryError.Transient(message: f"attempt \${attempts}"))
}

retry [0ms, 0ms] {
  flaky()?
}?
""",
  )?

  test.eq(output.status, 3)?
  test.contains(output.stderr, "attempt 3")?
  test.contains(output.stderr, "traceback")?
}

test test_retry_attempt_defers_run_before_next_attempt [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
var attempts = 0
var cleaned = 0

error RetryError = Transient(message: Str)

proc mark_cleaned() -> Result[Unit] {
  cleaned += 1
}

proc flaky() -> Result[Str] {
  attempts += 1
  if attempts < 2 {
    return Err(RetryError.Transient(message: "not yet"))
  }
  return Ok("ok")
}

let value = retry [0ms] {
  defer mark_cleaned()?
  flaky()?
}?
print f"\${value} \${attempts} \${cleaned}"
""",
  )?

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """ok 2 2
""",
  )?

  test.eq(output.stderr, "")?
}

test test_return_inside_retry_returns_from_enclosing_proc [error] { |ctx|
  let output = test.run_script(
    ctx,
    """
proc main() -> Result[Str] {
  let value = retry [] {
    return Ok("outer")
  }?
  Ok("after")
}

let result = main()?
print \${result}
""",
  )?

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """outer
""",
  )?

  test.eq(output.stderr, "")?
}

test test_retry_attempts_are_traced [error] { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  if attempts < 2 {
    return Err(RetryError.Transient(message: "not yet"))
  }
  return Ok("ok")
}

let value = retry [0ms] {
  flaky()?
}?
print \${value}
""",
    ["--raw", "--trace-format", "jsonl"],
  )?

  test.ok(output.success, output.stderr)?

  test.eq(
    output.stdout,
    """ok
""",
  )?

  test.contains(output.stderr, "\"kind\":\"retry.attempt\"")?
  test.contains(output.stderr, "\"attempt\":1")?
  test.contains(output.stderr, "\"attempt\":2")?
  test.contains(output.stderr, "\"next_delay_ms\":0")?
  test.contains(output.stderr, "\"kind\":\"RetryError.Transient\"")?
}

proc test_retry_filter_stops_on_first_nonmatching_error(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FetchError = Busy(message: Str) | Fatal(message: Str)
var attempts = 0
var delays = 0
var cleaned = 0
proc delay() -> Duration { delays += 1; 0ms }
proc cleanup() -> Result[Unit] { cleaned += 1 }
let result = retry [delay(), delay()] on (FetchError.Busy) {
  defer cleanup()?
  attempts += 1
  Err(FetchError.Fatal(message: "original"))?
}
match result {
  Err(FetchError.Fatal {message}) => print \${message}
  _ => print "wrong"
}
print f"\${attempts} \${delays} \${cleaned}"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "original\n1 2 1\n")?
}

proc test_retry_filter_matching_exhaustion_and_mixed_failures(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FetchError = Busy(message: Str) | Timeout(message: Str) | Fatal(message: Str)
var attempts = 0
proc exhausted() -> Result[Str, FetchError] {
  attempts += 1
  Err(FetchError.Busy(message: f"attempt \${attempts}"))
}
let last = retry [0ms, 0ms] on (FetchError.Busy | FetchError.Timeout) { exhausted()? }
match last {
  Err(FetchError.Busy {message}) => print \${message}
  _ => print "wrong"
}
attempts = 0
proc mixed() -> Result[Str, FetchError] {
  attempts += 1
  if attempts == 1 { return Err(FetchError.Timeout(message: "first")) }
  Err(FetchError.Fatal(message: "second"))
}
let result = retry [0ms, 0ms] on (FetchError.Busy | FetchError.Timeout) { mixed()? }
match result {
  Err(FetchError.Fatal {message}) => print \${message}
  _ => print "wrong"
}
print \${attempts}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "attempt 3\nsecond\n2\n")?
}

proc test_retry_filter_empty_delays_success_and_nested_retry(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FetchError = Busy(message: Str) | Fatal(message: Str)
var attempts = 0
proc attempt() -> Result[Str, FetchError] {
  attempts += 1
  Err(FetchError.Busy(message: "last"))
}
let once = retry [] on (FetchError.Busy) { attempt()? }
print \${attempts}
let good = retry [0ms] on (_) { "ok" }?
print \${good}
attempts = 0
let nested = retry [0ms] on (FetchError.Busy) {
  retry [] on (FetchError.Busy) { attempt()? }?
}
print \${attempts}
match nested { Err(FetchError.Busy {message}) => print \${message}; _ => print "wrong" }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1\nok\n2\nlast\n")?
}

proc test_retry_filter_trace_records_selection_and_stop_reason(ctx: TestContext) [error] {
  let output = test.run_xsht_trace(ctx, """
error FetchError = Busy(message: Str) | Fatal(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Fatal(message: "original")) }
let result = retry [0ms] on (FetchError.Busy) { attempt()? }
match result { Err(_) => print "stopped"; _ => print "wrong" }
""", ["--raw", "--trace-format", "jsonl"])?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "stopped\n")?
  test.contains(output.stderr, "\"kind\":\"retry.attempt\"")?
  test.contains(output.stderr, "\"selected\":false")?
  test.contains(output.stderr, "\"stop_reason\":\"nonmatching\"")?
}

proc test_retry_filter_rejects_captures_and_impossible_families(ctx: TestContext) [error] {
  let capture = test.run_script(ctx, """
error FetchError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (FetchError.Busy {message}) { attempt()? }
""")?
  test.eq(capture.status, 2)?
  test.contains(capture.stderr, "check.pattern-test-binding")?
  let impossible = test.run_script(ctx, """
error FetchError = Busy(message: Str)
error OtherError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (OtherError.Busy) { attempt()? }
""")?
  test.eq(impossible.status, 2)?
  test.contains(impossible.stderr, "check.pattern-type")?
}

proc test_retry_filter_facets_and_cleanup_failure_priority(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FetchError = Busy(message: Str) : NotFound | Fatal(message: Str) : InvalidData
var attempts = 0
var cleaned = 0
proc attempt() -> Result[Str, FetchError] {
  attempts += 1
  Err(FetchError.Busy(message: "primary"))
}
proc cleanup() -> Result[Unit, FetchError] {
  cleaned += 1
  Err(FetchError.Fatal(message: "secondary"))
}
let result = retry [0ms] on (NotFound) {
  defer cleanup()?
  attempt()?
}
match result { Err(FetchError.Busy {message}) => print \${message}; _ => print "wrong" }
print f"\${attempts} \${cleaned}"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "primary\n2 2\n")?
  test.contains(output.stderr, "secondary")?
}

proc test_retry_filter_does_not_retry_abort(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
let result = retry [0ms] on (_) {
  print "attempt"
  abort(7)
}
print "after"
""")?
  test.eq(output.status, 7)?
  test.eq(output.stdout, "attempt\n")?
}

proc test_retry_filter_alias_and_impossible_facet_diagnostics(ctx: TestContext) [error] {
  for source in [
    "error FetchError = Busy(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nlet result = retry [] on (FetchError.Busy as failure) { attempt()? }\n",
    "error FetchError = Busy(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nlet result = retry [] on (NotFound) { attempt()? }\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.eq(output.status, 2)?
    test.contains(output.stderr, "check.pattern-")?
  }
}

proc test_retry_filter_cleanup_failure_becomes_attempt_error(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FetchError = Busy(message: Str) | Fatal(message: Str)
var cleaned = 0
proc cleanup() -> Result[Unit, FetchError] {
  cleaned += 1
  Err(FetchError.Fatal(message: "cleanup"))
}
let result = retry [0ms] on (FetchError.Busy) {
  defer cleanup()?
  "success"
}
match result { Err(FetchError.Fatal {message}) => print \${message}; _ => print "wrong" }
print \${cleaned}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "cleanup\n1\n")?
}

proc test_retry_filter_rejects_string_classification(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
proc attempt() -> Result[Str, Str] { Err("busy") }
let result = retry [] on ("busy") { attempt()? }
""")?
  test.eq(output.status, 2)?
  test.contains(output.stderr, "check.retry-pattern")?
}

proc test_retry_filter_nested_try_and_lexical_return_keep_destinations(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
error FetchError = Busy(message: Str) | Fatal(message: Str)
var attempts = 0
proc busy() -> Result[Str, FetchError] {
  attempts += 1
  Err(FetchError.Busy(message: "retry"))
}
let result = retry [0ms] on (FetchError.Busy) {
  let local = try {
    Err(FetchError.Fatal(message: "local"))?
    "unreachable"
  }
  match local { Err(FetchError.Fatal {message}) => print \${message}; _ => print "wrong" }
  busy()?
}
print \${attempts}
proc escaped() -> Result[Str, FetchError] {
  let result = retry [0ms] on (FetchError.Busy) {
    return Err(FetchError.Fatal(message: "escape"))
  }
  "wrong"
}
match escaped() { Err(FetchError.Fatal {message}) => print \${message}; _ => print "wrong" }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "local\nlocal\n2\nescape\n")?
}
