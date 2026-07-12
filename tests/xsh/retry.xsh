proc test_retry_repeats_until_attempt_succeeds(ctx: TestContext) [error] {
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

let value = retry [0ms, 0ms] {
  flaky()?
}?
print f"\${value} \${attempts}"
""",
  )?

  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "done 3\n")?
  test.eq(output.stderr, "")?
}

proc test_retry_exhaustion_returns_final_error(ctx: TestContext) [error] {
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

proc test_retry_attempt_defers_run_before_next_attempt(ctx: TestContext) [error] {
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
  test.eq(output.stdout, "ok 2 2\n")?
  test.eq(output.stderr, "")?
}

proc test_return_inside_retry_returns_from_enclosing_proc(ctx: TestContext) [error] {
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
  test.eq(output.stdout, "outer\n")?
  test.eq(output.stderr, "")?
}

proc test_retry_attempts_are_traced(ctx: TestContext) [error] {
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
  test.eq(output.stdout, "ok\n")?
  test.contains(output.stderr, "\"kind\":\"retry.attempt\"")?
  test.contains(output.stderr, "\"attempt\":1")?
  test.contains(output.stderr, "\"attempt\":2")?
  test.contains(output.stderr, "\"next_delay_ms\":0")?
  test.contains(output.stderr, "\"kind\":\"RetryError.Transient\"")?
}
