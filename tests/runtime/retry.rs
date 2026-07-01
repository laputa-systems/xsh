use super::common::*;

#[test]
fn retry_repeats_until_attempt_succeeds() {
    let output = run_temp_script(
        "retry-success",
        r#"
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  if attempts < 3 {
    return Err(RetryError.Transient(message: f"attempt ${attempts}"))
  }
  return Ok("done")
}

let value = retry [0ms, 0ms] {
  flaky()?
}?
print f"${value} ${attempts}"
"#,
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "done 3\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn retry_exhaustion_returns_final_error() {
    let output = run_temp_script(
        "retry-exhausted",
        r#"
var attempts = 0

error RetryError = Transient(message: Str)

proc flaky() -> Result[Str] {
  attempts += 1
  return Err(RetryError.Transient(message: f"attempt ${attempts}"))
}

retry [0ms, 0ms] {
  flaky()?
}?
"#,
    );

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("attempt 3"), "{stderr}");
    assert!(stderr.contains("traceback"), "{stderr}");
}

#[test]
fn retry_attempt_defers_run_before_next_attempt() {
    let output = run_temp_script(
        "retry-defers",
        r#"
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
print f"${value} ${attempts} ${cleaned}"
"#,
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok 2 2\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn return_inside_retry_returns_from_enclosing_proc() {
    let output = run_temp_script(
        "retry-return",
        r#"
proc main() -> Result[Str] {
  let value = retry [] {
    return Ok("outer")
  }?
  Ok("after")
}

let result = main()?
print ${result}
"#,
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "outer\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn retry_attempts_are_traced() {
    let output = run_temp_script_with_args(
        "retry-trace",
        r#"
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
print ${value}
"#,
        ["--trace", "--raw", "--trace-format", "jsonl"],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ok\n");
    let trace = String::from_utf8(output.stderr).unwrap();
    assert!(trace.contains("\"kind\":\"retry.attempt\""), "{trace}");
    assert!(trace.contains("\"attempt\":1"), "{trace}");
    assert!(trace.contains("\"attempt\":2"), "{trace}");
    assert!(trace.contains("\"next_delay_ms\":0"), "{trace}");
    assert!(
        trace.contains("\"kind\":\"RetryError.Transient\""),
        "{trace}"
    );
}
