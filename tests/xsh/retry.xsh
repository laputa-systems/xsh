test test_retry_repeats_until_attempt_succeeds { |ctx|
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  output.stdout == """done 3
"""

  output.stderr == ""
}

test test_retry_exhaustion_returns_final_error { |ctx|
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

  output.status == 3
  "attempt 3" in output.stderr
  "traceback" in output.stderr
}

test test_retry_attempt_defers_run_before_next_attempt { |ctx|
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  output.stdout == """ok 2 2
"""

  output.stderr == ""
}

test test_return_inside_retry_returns_from_enclosing_proc { |ctx|
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  output.stdout == """outer
"""

  output.stderr == ""
}

test test_retry_attempts_are_traced { |ctx|
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

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  output.stdout == """ok
"""

  "\"kind\":\"retry.attempt\"" in output.stderr
  "\"attempt\":1" in output.stderr
  "\"attempt\":2" in output.stderr
  "\"next_delay_ms\":0" in output.stderr
  "\"kind\":\"RetryError.Transient\"" in output.stderr
}

test test_retry_filter_stops_on_first_nonmatching_error { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("original\n1 2 1\n")
}

test test_retry_filter_matching_exhaustion_and_mixed_failures { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("attempt 3\nsecond\n2\n")
}

test test_retry_filter_empty_delays_success_and_nested_retry { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("1\nok\n2\nlast\n")
}

test test_retry_filter_trace_records_selection_and_stop_reason { |ctx|
  let output = test.run_xsht_trace(ctx, """
error FetchError = Busy(message: Str) | Fatal(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Fatal(message: "original")) }
let result = retry [0ms] on (FetchError.Busy) { attempt()? }
match result { Err(_) => print "stopped"; _ => print "wrong" }
""", ["--raw", "--trace-format", "jsonl"])?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("stopped\n")
  ("\"kind\":\"retry.attempt\"" in output.stderr)
  ("\"selected\":false" in output.stderr)
  ("\"stop_reason\":\"nonmatching\"" in output.stderr)
}

test test_retry_filter_rejects_captures_and_impossible_families { |ctx|
  let capture = test.run_script(ctx, """
error FetchError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (FetchError.Busy {message}) { attempt()? }
""")?
  (capture.status) == (2)
  ("check.pattern-test-binding" in capture.stderr)
  let impossible = test.run_script(ctx, """
error FetchError = Busy(message: Str)
error OtherError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (OtherError.Busy) { attempt()? }
""")?
  (impossible.status) == (2)
  ("check.pattern-type" in impossible.stderr)
}

test test_retry_filter_facets_and_cleanup_failure_priority { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("primary\n2 2\n")
  ("secondary" in output.stderr)
}

test test_retry_filter_does_not_retry_abort { |ctx|
  let output = test.run_script(ctx, """
let result = retry [0ms] on (_) {
  print "attempt"
  abort(7)
}
print "after"
""")?
  (output.status) == (7)
  (output.stdout) == ("attempt\n")
}

test test_retry_filter_alias_and_impossible_facet_diagnostics { |ctx|
  for source in [
    "error FetchError = Busy(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nlet result = retry [] on (FetchError.Busy as failure) { attempt()? }\n",
    "error FetchError = Busy(message: Str)\nproc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: \"busy\")) }\nlet result = retry [] on (NotFound) { attempt()? }\n",
  ] {
    let output = test.run_script(ctx, source)?
    (output.status) == (2)
    ("check.pattern-" in output.stderr)
  }
}

test test_retry_filter_cleanup_failure_becomes_attempt_error { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("cleanup\n1\n")
}

test test_retry_filter_rejects_string_classification { |ctx|
  let output = test.run_script(ctx, """
proc attempt() -> Result[Str, Str] { Err("busy") }
let result = retry [] on ("busy") { attempt()? }
""")?
  (output.status) == (2)
  ("check.retry-pattern" in output.stderr)
}

test test_retry_filter_nested_try_and_lexical_return_keep_destinations { |ctx|
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
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("local\nlocal\n2\nescape\n")
}
