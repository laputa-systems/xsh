test test_retry_repeats_until_attempt_succeeds { |ctx|
  let output = test.run_script(
    ctx,
    """
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  if attempts < 3 {
    return Err(RetryError.Transient(message: f"attempt {attempts}"))
  }
  return Ok("done")
}

let value = retry [1ms / 2, 0ms * 2] {
  flaky()?
}?
print f"{value} {attempts}"
""",
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  assert output.stdout == """done 3
"""

  assert output.stderr == ""
}

test test_retry_exhaustion_returns_final_error { |ctx|
  test.expect(
    ctx,
    """
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  return Err(RetryError.Transient(message: f"attempt {attempts}"))
}

let _ = retry [0ms, 0ms] {
  flaky()?
}?
""",
    status: 3,
    stderr: ["attempt 3", "err: RetryError.Transient: attempt 3"],
  )?
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
print f"{value} {attempts} {cleaned}"
""",
  )?

  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }

  assert output.stdout == """ok 2 2
"""

  assert output.stderr == ""
}

test test_yield_inside_retry_attempt_is_a_checker_error { |ctx|
  # An attempt runs outside its producer's frame: a `yield` there used to
  # check and then fail at runtime. A `try` block keeps yielding.
  let rejected = test.run_script(
    ctx,
    """stream items() [] -> Stream[Int] {
  let attempt: Result[Int] = retry [] {
    yield 1
    2
  }
  yield attempt ?? 0
}
for item in items() { print $item }
""",
  )?
  assert ! rejected.success
  assert "check.yield" in rejected.stderr, rejected.stderr
  assert rejected.stdout == ""
  let captured = test.expect(
    ctx,
    """stream items() [] -> Stream[Int] {
  let attempt = try {
    yield 1
    2
  }
  yield attempt ?? 0
}
for item in items() { print $item }
""",
    status: 0,
  )?
  assert captured.stdout == "1\n2\n"
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

  assert output.stdout == """outer
"""

  assert output.stderr == ""
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

  assert output.stdout == """ok
"""

  assert "\"kind\":\"retry.attempt\"" in output.stderr
  assert "\"attempt\":1" in output.stderr
  assert "\"attempt\":2" in output.stderr
  assert "\"next_delay_ms\":0" in output.stderr
  assert "\"kind\":\"RetryError.Transient\"" in output.stderr
}

test test_retry_filter_stops_on_first_nonmatching_error { |ctx|
  let output = test.run_script(
    ctx,
    """
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
print f"{attempts} {delays} {cleaned}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """original
1 2 1
"""
}

test test_retry_filter_matching_exhaustion_and_mixed_failures { |ctx|
  let output = test.run_script(
    ctx,
    """
error FetchError = Busy(message: Str) | Timeout(message: Str) | Fatal(message: Str)
var attempts = 0
proc exhausted() -> Result[Str, FetchError] {
  attempts += 1
  Err(FetchError.Busy(message: f"attempt {attempts}"))
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """attempt 3
second
2
"""
}

test test_retry_filter_empty_delays_success_and_nested_retry { |ctx|
  let output = test.run_script(
    ctx,
    """
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """1
ok
2
last
"""
}

test test_retry_filter_trace_records_selection_and_stop_reason { |ctx|
  let output = test.run_xsht_trace(
    ctx,
    """
error FetchError = Busy(message: Str) | Fatal(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Fatal(message: "original")) }
let result = retry [0ms] on (FetchError.Busy) { attempt()? }
match result { Err(_) => print "stopped"; _ => print "wrong" }
""",
    ["--raw", "--trace-format", "jsonl"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """stopped
"""
  assert "\"kind\":\"retry.attempt\"" in output.stderr
  assert "\"selected\":false" in output.stderr
  assert "\"stop_reason\":\"nonmatching\"" in output.stderr
}

test test_retry_filter_rejects_captures_and_impossible_families { |ctx|
  test.expect(
    ctx,
    """
error FetchError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (FetchError.Busy {message}) { attempt()? }
""",
    status: 2,
    stderr: ["check.pattern-test-binding"],
  )?
  test.expect(
    ctx,
    """
error FetchError = Busy(message: Str)
error OtherError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (OtherError.Busy) { attempt()? }
""",
    status: 2,
    stderr: ["check.pattern-type"],
  )?
}

test test_retry_filter_facets_and_cleanup_failure_priority { |ctx|
  let output = test.run_script(
    ctx,
    """
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
print f"{attempts} {cleaned}"
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """primary
2 2
"""
  assert "secondary" in output.stderr
}

test test_retry_filter_does_not_retry_abort { |ctx|
  let output = test.expect(
    ctx,
    """
let result = retry [0ms] on (_) {
  print "attempt"
  exit 7
}
print "after"
""",
    status: 7,
  )?
  assert output.stdout == """attempt
"""
}

test test_retry_filter_alias_and_impossible_facet_diagnostics { |ctx|
  for source in [
    """error FetchError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (FetchError.Busy as failure) { attempt()? }
""",
    """error FetchError = Busy(message: Str)
proc attempt() -> Result[Str, FetchError] { Err(FetchError.Busy(message: "busy")) }
let result = retry [] on (NotFound) { attempt()? }
""",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["check.pattern-"])?
  }
}

test test_retry_filter_cleanup_failure_becomes_attempt_error { |ctx|
  let output = test.run_script(
    ctx,
    """
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """cleanup
1
"""
}

test test_retry_filter_rejects_string_classification { |ctx|
  test.expect(
    ctx,
    """
proc attempt() -> Result[Str, Str] { Err("busy") }
let result = retry [] on ("busy") { attempt()? }
""",
    status: 2,
    stderr: ["check.retry-pattern"],
  )?
}

test test_retry_filter_nested_try_and_lexical_return_keep_destinations { |ctx|
  let output = test.run_script(
    ctx,
    """
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """local
local
2
escape
"""
}

test test_retry_body_uses_the_expected_success_type {
  let counts: Result[Map[Str, Int]] = retry [] {
    {}
  }
  let names: Result[List[Str]] = retry [] {
    []
  }
  assert counts?.len() == 0
  assert names?.len() == 0
}

test test_retry_attempt_local_try_needs_no_error_effect { |ctx|
  test.expect(
    ctx,
    r"""
proc fetch() [net] -> Result[Str] {
  return Ok("ok")
}

proc main() [net] -> Unit {
  let value = retry [] {
    fetch()?
  }
}
""",
    status: 0,
  )?
}

test test_retry_delays_require_the_time_effect { |ctx|
  test.expect(
    ctx,
    r"""
proc main() [fs] -> Unit {
  let value = retry [1ms] {
    Ok("ok")
  }
}
""",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_retry_rejects_non_duration_delays { |ctx|
  test.expect(
    ctx,
    r"""
let value = retry ["soon"] {
  Ok("ok")
}
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_retry_family_selectors_keep_builtin_and_user_error_identity { |ctx|
  for source in [
    "let result: Result[Unit, AssertionError] = retry [] on (AssertionError) { assert false, \"condition\" }\n",
    "error LocalError = Failed(message: Str)\nlet result = retry [] on (LocalError) { Err(LocalError.Failed(message: \"local\"))? }\n",
    "let result = retry [] on (Error) { assert false, \"condition\" }\n",
    "let result = retry [] on (ProcessError) { Err(ProcessError.NotFound(message: \"missing\", status: null))? }\n",
  ] {
    let file = test.temp_file(ctx, name: "selector.xsh", contents: bytes.from_text(source))?
    let checked = run.capture --text "xsht" check $file
    assert checked.status.exited_with(0), f"{source}: {checked.stderr}"
    assert "[check." not in checked.stderr, f"{source}: {checked.stderr}"
  }

  for source in [
    "error OtherError = Failed(message: Str)\nlet result: Result[Unit, AssertionError] = retry [] on (OtherError) { assert false, \"condition\" }\n",
    "error LocalError = Failed(message: Str)\nlet failed: Result[Unit, LocalError] = Err(LocalError.Failed(message: \"local\"))\nlet result = retry [] on (AssertionError) { failed? }\n",
    "let value = 1\nlet tested = value is Str\n",
  ] {
    test.expect(ctx, source, status: 2, stderr: ["[check.pattern-type]"])?
  }
}
