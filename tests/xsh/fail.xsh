pure port(value: Int) -> Result[Int] {
  if value < 1 or value > 65535 {
    fail f"port {value} is out of range"
  }

  Ok(value)
}

test test_fail_returns_an_error_with_the_message {
  assert port(8080)? == 8080
  match port(0) {
    Ok(value) => test.fail(f"port 0 was accepted as {value}")
    Err(problem) => assert problem.message == "port 0 is out of range"
  }
}

pure classify(size: Int) -> Result[Str] {
  fail "negative size" when size < 0
  fail "too large" unless size < 1000
  guard size != 13 else { fail "unlucky" }
  match size {
    0 => fail "empty"
    else => Ok(f"{size} bytes")
  }
}

pure message_of(outcome: Result[Str]) -> Str {
  match outcome {
    Ok(text) => f"ok: {text}"
    Err(problem) => problem.message
  }
}

test test_fail_takes_a_postfix_guard_and_ends_a_block {
  assert message_of(classify(-1)) == "negative size"
  assert message_of(classify(5000)) == "too large"
  assert message_of(classify(13)) == "unlucky"
  assert message_of(classify(0)) == "empty"
  assert message_of(classify(7)) == "ok: 7 bytes"
}

pure rewrap(outcome: Result[Int]) -> Result[Int] {
  match outcome {
    Ok(value) => Ok(value)
    # `error` is the caught error here, and `fail` still builds a new one.
    Err(error) => fail f"rewrapped: {error.message}"
  }
}

test test_fail_means_the_same_where_error_is_bound {
  match rewrap(port(0)) {
    Ok(value) => test.fail(f"rewrapped a failure as {value}")
    Err(problem) => assert problem.message == "rewrapped: port 0 is out of range"
  }
}

test test_fail_leaves_the_function_and_runs_cleanup { |ctx|
  let source = "proc stage(ready: Bool) -> Result[Int] {\n  defer { print \"cleanup\" }\n  let inner = try {\n    fail \"not ready\" unless ready\n    1\n  }\n  print \"after try\"\n  Ok(2)\n}\n\nmatch stage(false) {\n  Ok(_) => print \"ok\"\n  Err(problem) => print $problem.message\n}\nprint f\"{stage(true)?}\"\n"
  let output = test.run_script(ctx, source)?
  assert output.success, output.stderr
  assert output.stdout == "cleanup\nnot ready\nafter try\ncleanup\n2\n", output.stdout
}

test test_an_uncaught_fail_reports_its_message_and_statement { |ctx|
  let output = test.run_script(
    ctx,
    "proc load() -> Result[Int] {\n  fail \"no configuration\"\n}\n\nlet value = load()?\n",
  )?
  assert output.status == 3, output.stderr
  assert "no configuration" in output.stderr, output.stderr
}

test test_fail_is_not_a_reserved_word { |ctx|
  let source = "proc fail(code: Int) -> Int {\n  code + 1\n}\n\nvar fail_count = 0\nlet options = {fail: true}\nlet fail = 3\nprint f\"{fail + 1} {options.fail}\"\n"
  let output = test.run_script(ctx, source)?
  assert output.success, output.stderr
  assert output.stdout == "4 true\n", output.stdout
  let called = test.run_script(ctx, "proc fail(code: Int) -> Int {\n  code + 1\n}\n\nprint f\"{fail(1)}\"\n")?
  assert called.stdout == "2\n", called.stderr
}

test test_fail_carries_the_diagnostics_of_its_return { |ctx|
  let wrong_message = test.run_script(ctx, "proc load() -> Result[Int] {\n  fail 404\n}\n")?
  assert ! wrong_message.success
  assert "err[check.type-mismatch]" in wrong_message.stderr, wrong_message.stderr
  assert "expected Str, found Int" in wrong_message.stderr, wrong_message.stderr
  let not_fallible = test.run_script(ctx, "proc load() -> Int {\n  fail \"no value\"\n}\n")?
  assert ! not_fallible.success
  assert "expected Int, found Result[" in not_fallible.stderr, not_fallible.stderr
  let top_level = test.run_script(ctx, "fail \"no input\"\n")?
  assert ! top_level.success
  assert "err[check.return-outside-callable]" in top_level.stderr, top_level.stderr
  let bare = test.run_script(ctx, "proc load() -> Result[Int] {\n  fail\n}\n")?
  assert ! bare.success
  assert "unresolved proc command `fail`" in bare.stderr, bare.stderr
}

test test_error_failure_is_the_error_fail_wraps {
  let built = error.failure("disk full")
  assert built.message == "disk full"
  match error.fail("disk full") {
    Ok(_) => test.fail("error.fail produced Ok")
    Err(problem) => assert problem.message == built.message
  }
}

test test_fmt_desugar_and_highlight_know_the_fail_statement { |ctx|
  let source = "proc load(ready: Bool) -> Result[Int] {\n  fail   \"not ready\"   unless ready\n  Ok(1)\n}\n"
  let file = test.temp_file(ctx, name: "fails.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $file ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == "proc load(ready: Bool) -> Result[Int] {\n  fail \"not ready\" unless ready\n  Ok(1)\n}\n"
  let shown = run.capture --text "xsht" highlight $file ?
  assert r"""{"kind":"keyword","text":"fail"}""" in shown.stdout, shown.stdout
  let expanded = run.capture --text "xsht" desugar $file ?
  assert expanded.status.exited_with(0), expanded.stderr
  assert "    return Err(error.failure(\"not ready\"))" in expanded.stdout, expanded.stdout
}

test test_prefer_fail_fix_keeps_the_messages_and_converges { |ctx|
  let source = r"""error StageError = Failed(message: Str)

proc stage(name: Str) -> Result[Str] {
  return Err(StageError.Failed("empty name")) when name == ""
  if name.starts_with("-") {
    return Err(StageError.Failed(message: f"{name} is an option"))
  }

  Ok(name.upper())
}

for name in ["", "-v", "ok"] {
  match stage(name) {
    Ok(value) => print $value
    Err(problem) => print $problem.message
  }
}
"""
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "stage.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.prefer-fail $candidate ?
  assert first.status.exited_with(1), first.stderr
  assert "`StageError.Failed` only carries a message; report it with `fail`" in first.stderr, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-fail $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert fixed.starts_with("proc stage(name: Str) -> Result[Str] {\n  fail \"empty name\" when name == \"\"\n"), fixed
  assert "    fail f\"{name} is an option\"\n" in fixed, fixed
  assert "StageError" not in fixed, fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let second = run.capture --text "xsht" lint --only lint.prefer-fail $candidate ?
  assert second.status.exited_with(0), second.stderr
}

test test_prefer_fail_leaves_a_matched_family_alone { |ctx|
  let source = "error StageError = Failed(message: Str)\n\nproc stage() -> Result[Str, StageError] {\n  return Err(StageError.Failed(\"no\"))\n}\n\nmatch stage() {\n  Ok(value) => print $value\n  Err(StageError.Failed {message}) => print $message\n}\n"
  let candidate = test.temp_file(ctx, name: "matched.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text "xsht" lint --only lint.prefer-fail $candidate ?
  assert linted.status.exited_with(0), linted.stderr
}
