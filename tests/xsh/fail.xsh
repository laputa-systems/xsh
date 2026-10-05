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
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "cleanup\nnot ready\nafter try\ncleanup\n2\n", output.stdout
}

test test_an_uncaught_fail_reports_its_message_and_statement { |ctx|
  test.expect(
    ctx,
    "proc load() -> Result[Int] {\n  fail \"no configuration\"\n}\n\nlet value = load()?\n",
    status: 3,
    stderr: ["no configuration"],
  )?
}

test test_fail_is_not_a_reserved_word { |ctx|
  let source = "proc fail(code: Int) -> Int {\n  code + 1\n}\n\nvar fail_count = 0\nlet options = {fail: true}\nlet fail = 3\nprint f\"{fail + 1} {options.fail}\"\n"
  let output = test.expect(ctx, source, status: 0)?
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
  # The misplaced statement is the whole report.
  assert top_level.stderr.split("err[").len() == 2, top_level.stderr
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
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == "proc load(ready: Bool) -> Result[Int] {\n  fail \"not ready\" unless ready\n  Ok(1)\n}\n"
  let shown = run.capture --text "xsht" highlight $file
  assert r"""{"kind":"keyword","text":"fail"}""" in shown.stdout, shown.stdout
  let arms = test.temp_file(
    ctx,
    name: "arms.xsh",
    contents: bytes.from_text("match code {\n  0 => exit 3\n  else => fail \"odd\"\n}\n"),
  )?
  let painted = run.capture --text "xsht" highlight $arms
  assert r"""{"kind":"keyword","text":"exit"}""" in painted.stdout, painted.stdout
  assert r"""{"kind":"keyword","text":"fail"}""" in painted.stdout, painted.stdout
  let expanded = run.capture --text "xsht" desugar $file
  assert expanded.status.exited_with(0), expanded.stderr
  assert "    return Err(error.failure(\"not ready\"))" in expanded.stdout, expanded.stdout
}

test test_prefer_fail_fix_keeps_the_messages_and_converges { |ctx|
  let source = r"""# Stages names.
# Usage: stage NAME
error StageError = Failed(message: Str)

const prefix = "-"

proc stage(name: Str) -> Result[Str] {
  return Err(StageError.Failed("empty name")) when name == ""
  if name.starts_with(prefix) {
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
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "stage.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.prefer-fail $candidate
  assert first.status.exited_with(1), first.stderr
  assert "`StageError.Failed` only carries a message; report it with `fail`" in first.stderr, first.stderr
  # One report for each constructor; the declaration is reported once they
  # are gone, and one `--fix` runs both steps.
  assert first.stderr.split("warn[lint.prefer-fail]").len() == 3, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-fail $candidate
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert fixed.starts_with(
    "# Stages names.\n# Usage: stage NAME\n\nconst prefix = \"-\"\n\nproc stage(name: Str) -> Result[Str] {\n  fail \"empty name\" when name == \"\"\n",
  ), fixed
  assert "    fail f\"{name} is an option\"\n" in fixed, fixed
  assert "StageError" not in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let second = run.capture --text "xsht" lint --only lint.prefer-fail $candidate
  assert second.status.exited_with(0), second.stderr
}

test test_prefer_fail_leaves_a_matched_family_alone { |ctx|
  let source = "error StageError = Failed(message: Str)\n\nproc stage() -> Result[Str, StageError] {\n  return Err(StageError.Failed(\"no\"))\n}\n\nmatch stage() {\n  Ok(value) => print $value\n  Err(StageError.Failed {message}) => print $message\n}\n"
  let candidate = test.temp_file(ctx, name: "matched.xsh", contents: bytes.from_text(source))?
  let linted = run.capture --text "xsht" lint --only lint.prefer-fail $candidate
  assert linted.status.exited_with(0), linted.stderr
}

error FetchError = RemoteFetch | Offline

proc download(url: Str) -> Result[Str] {
  fail f"no route to {url}" unless url.starts_with("https:")
  Ok("body")
}

proc fetch(url: Str) -> Result[Str, FetchError] {
  fail .Offline() when url == ""
  match download(url) {
    Ok(body) => Ok(body)
    Err(problem) => fail .RemoteFetch(f"fetching {url}") because problem
  }
}

pure fetched(outcome: Result[Str, FetchError]) -> Str {
  match outcome {
    Ok(body) => body
    Err(.RemoteFetch {message}) => f"remote: {message}"
    Err(.Offline) => "offline"
    Err(other) => f"unexpected: {other.message}"
  }
}

test test_fail_returns_a_variant_of_the_declared_family {
  assert fetched(fetch("https://mirror")) == "body"
  assert fetched(fetch("")) == "offline"
  assert fetched(fetch("ftp://mirror")) == "remote: fetching ftp://mirror"
}

test test_fail_because_keeps_the_replaced_error_as_the_cause { |ctx|
  let source = r"""error FetchError = RemoteFetch | Offline

proc download(url: Str) -> Result[Str] {
  fail f"no route to {url}"
}

proc fetch(url: Str) -> Result[Str, FetchError] {
  match download(url) {
    Ok(body) => Ok(body)
    Err(problem) => fail .RemoteFetch(f"fetching {url}") because problem
  }
}

proc load(url: Str) -> Result[Str] {
  match fetch(url) {
    Ok(body) => Ok(body)
    # `error` is the caught error, as a cause and beside the new message.
    Err(error) => fail f"loading {url}: {error.message}" because error
  }
}

match load("ftp://mirror") {
  Ok(body) => print $body
  Err(problem) => print $problem.message
}
print load("ftp://mirror")?
"""
  let output = test.expect(ctx, source, status: 3)?
  assert output.stdout == "loading ftp://mirror: fetching ftp://mirror\n", output.stdout
  let report = output.stderr.lines()
  let outer = [line for line in report if line.starts_with("err: ")]
  let causes = [line for line in report if line.starts_with("caused by: ")]
  assert outer.len() == 1 and "loading ftp://mirror: fetching ftp://mirror" in outer[0], output.stderr
  assert causes.len() == 2, output.stderr
  assert "FetchError.RemoteFetch" in causes[0] and "fetching ftp://mirror" in causes[0], output.stderr
  assert "no route to ftp://mirror" in causes[1], output.stderr
}

test test_fail_variant_needs_a_declared_family_and_a_cause_an_error { |ctx|
  let undeclared = test.run_script(
    ctx,
    "error FetchError = RemoteFetch | Offline\n\nproc fetch() -> Result[Str] {\n  fail .Offline()\n}\n",
  )?
  assert ! undeclared.success
  assert "err[check.inferred-variant]" in undeclared.stderr, undeclared.stderr
  # The advice is the one a `fail` statement can follow.
  assert "note: declare the family in the type, as in `Result[T, Family]`" in undeclared.stderr, undeclared.stderr
  let inferred = test.run_script(
    ctx,
    "error FetchError = RemoteFetch | Offline\n\nproc fetch() {\n  fail .Offline()\n}\n",
  )?
  assert ! inferred.success
  assert "err[check.inferred-variant]" in inferred.stderr, inferred.stderr
  let qualified = test.run_script(
    ctx,
    "error FetchError = RemoteFetch | Offline\n\nproc fetch() -> Result[Str, FetchError] {\n  fail FetchError.Offline()\n}\n",
  )?
  assert ! qualified.success
  assert "expected Str, found FetchError" in qualified.stderr, qualified.stderr
  let wrong_cause = test.run_script(
    ctx,
    "proc fetch() -> Result[Str] {\n  fail \"no\" because \"reasons\"\n}\n",
  )?
  assert ! wrong_cause.success
  assert "err[check." in wrong_cause.stderr, wrong_cause.stderr
  let missing_cause = test.run_script(ctx, "proc fetch() -> Result[Str] {\n  fail \"no\" because\n}\n")?
  assert ! missing_cause.success
  assert "err[parse." in missing_cause.stderr, missing_cause.stderr
}

test test_because_is_a_word_only_after_a_failure { |ctx|
  let source = "proc report(because: Error) -> Result[Int] {\n  let fail = {because: 1}\n  fail   because.message   because   because unless fail.because == 2\n  Ok(1)\n}\n"
  let file = test.temp_file(ctx, name: "because.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == "proc report(because: Error) -> Result[Int] {\n  let fail = {because: 1}\n  fail because.message because because unless fail.because == 2\n  Ok(1)\n}\n"
  let expanded = run.capture --text "xsht" desugar $file
  assert expanded.status.exited_with(0), expanded.stderr
  assert "return Err(error.failure(because.message), cause: because)" in expanded.stdout, expanded.stdout
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

test test_prefer_fail_respells_a_returned_leading_dot_error { |ctx|
  let source = "error LoadError = Missing(path: Path) | Busy\n\nproc load(target: Path, inner: Result[Int]) -> Result[Int, LoadError] {\n  return Err(.Busy()) when target.display() == \"\"\n  match inner {\n    Ok(value) => Ok(value)\n    Err(problem) => return Err(.Missing(path: target), cause: problem)\n  }\n}\n"
  let candidate = test.temp_file(ctx, name: "load.xsh", contents: bytes.from_text(source))?
  let first = run.capture --text "xsht" lint --only lint.prefer-fail $candidate
  assert first.status.exited_with(1), first.stderr
  assert "return an error written `.Variant(...)` with `fail`" in first.stderr, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-fail $candidate
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "  fail .Busy() when target.display() == \"\"\n" in fixed, fixed
  assert "    Err(problem) => fail .Missing(path: target) because problem\n" in fixed, fixed
  let formatted = run.capture --text "xsht" fmt --check $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let checked = run.capture --text "xsht" check $candidate
  assert checked.status.exited_with(0), checked.stderr
}

# Every rule's fixes are applied together, and an edit that overlaps another
# rule's is dropped for the round. Deleting the family must not count on a
# rewrite that `lint.prefer-guard` displaced.
test test_every_rule_fixing_one_file_leaves_it_checking { |ctx|
  let source = r"""error AppError = Failed(message: Str)

proc validate(argv: List[Str]) [] -> Result[Unit] {
  if argv.len() > 4 {
    return Err(AppError.Failed("too many"))
  }

  if argv.len() > 3 {
    return Err(AppError.Failed("this message is long enough that the one-line guard passes the column cap"))
  }

  for arg in argv {
    if arg == "" {
      return Err(AppError.Failed("empty"))
    }

    for part in arg.split(",") {
      if part == "" {
        return Err(AppError.Failed("empty part"))
      }
    }
  }
}

match validate(["a", "", "c"]) {
  Ok(_) => print "ok"
  Err(problem) => print $problem.message
}
"""
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "validate.xsh", contents: bytes.from_text(source))?
  let fixing = run.capture --text "xsht" lint --fix $candidate
  let fixed = candidate.read_text()?
  let checked = run.capture --text "xsht" check $candidate
  assert checked.status.exited_with(0), fixed + checked.stderr
  assert fixing.status.exited_with(0), fixing.stderr
  assert "AppError" not in fixed, fixed
  assert "  fail \"too many\" when argv.len() > 4\n" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
}

test test_prefer_fail_says_why_a_commented_family_is_left { |ctx|
  let source = r"""error StageError = Failed(message: Str) # legacy name

proc stage(name: Str) -> Result[Str] {
  return Err(StageError.Failed("empty name")) when name == ""
  Ok(name.upper())
}

print ${stage("ok")?}
"""
  let candidate = test.temp_file(ctx, name: "stage.xsh", contents: bytes.from_text(source))?
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-fail $candidate
  # The constructor is rewritten and the family is left, with the one report
  # that no fix answers.
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert fixed.starts_with("error StageError = Failed(message: Str) # legacy name\n"), fixed
  assert "  fail \"empty name\" when name == \"\"\n" in fixed, fixed
  let left = run.capture --text "xsht" lint --only lint.prefer-fail $candidate
  assert left.status.exited_with(1), left.stderr
  assert "error family `StageError` is never constructed" in left.stderr, left.stderr
  assert "note: `--fix` leaves this declaration: it has a comment beside it, and a fix never removes a comment. Delete the declaration by hand, with the comment if it describes the family" in left.stderr, left.stderr
  assert "help:" not in left.stderr, left.stderr
}
